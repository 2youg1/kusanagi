/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Async.TCP

/-!
# Something that answers a socket the way a host should not

`Kusanagi.Relay` stands in front of a real host and changes nothing. This stands
in the host's place and follows a script: hold the connection open and say
nothing, answer with bytes that are not a box's, or send the client somewhere
else. What it records is what a network adversary would learn — that a
connection arrived, and the request head it carried — so a property can say
"this listener was never connected to" and mean it.
-/

namespace Kusanagi.Listener

open Std.Net
open Std.Async (Async)
open Std.Async.TCP

/-- What to do with each connection that arrives. -/
inductive Script where
  /-- Accept, read the head, and never answer. -/
  | blackHole
  /-- Answer with exactly these bytes, then close. -/
  | answer (bytes : ByteArray)
  /-- Answer with a 302 to this locator, then close. -/
  | redirect (locator : String)
  deriving Inhabited

instance : BEq Script where
  beq
    | .blackHole, .blackHole => true
    | .answer left, .answer right => left.toList == right.toList
    | .redirect left, .redirect right => left == right
    | _, _ => false

instance : ToString Script where
  toString
    | .blackHole => "BlackHole"
    | .answer bytes => s!"Answer {bytes.toList}"
    | .redirect locator => s!"Redirect {locator}"

/--
A listener that is running, and everything a property may ask it afterwards.

The gate is held here rather than in `withListener` alone because this
toolchain's sockets have no `close`: a socket lives as long as something refers
to it, so the field is what keeps the gate open for the length of the action.
-/
structure Listener where
  /-- The gate the accept loop is sitting on. -/
  gate : Socket.Server
  /-- The port the operating system assigned, which nothing here chose. -/
  port : UInt16
  private arrived : IO.Ref Nat
  private seen : IO.Ref (List ByteArray)
  private held : IO.Ref (List Socket.Client)
  private running : IO.Ref Bool

/-- Where an endpoint is pointed to reach this listener as a host. -/
def Listener.locator (listener : Listener) : String :=
  s!"http://127.0.0.1:{listener.port}/"

/-- How many connections arrived. -/
def Listener.connections (listener : Listener) : IO Nat := listener.arrived.get

/-- Every request head that arrived, oldest first. -/
def Listener.heads (listener : Listener) : IO (List ByteArray) := do
  return (← listener.seen.get).reverse

/-- The loopback address at a port, which is the only place this ever listens. -/
private def loopback (port : UInt16) : SocketAddress :=
  .v4 { addr := IPv4Addr.ofParts 127 0 0 1, port }

private def swallow (act : IO Unit) : IO Unit := try act catch _ => pure ()

/-- Whether a blank line has arrived, which is where a request head ends. -/
private def headComplete (buffer : ByteArray) : Bool := Id.run do
  if buffer.size < 4 then
    return false
  for index in [0 : buffer.size - 3] do
    if buffer[index]? == some 13 && buffer[index + 1]? == some 10
        && buffer[index + 2]? == some 13 && buffer[index + 3]? == some 10 then
      return true
  return false

/--
Reads until the head is complete, the client stops talking, or 16 KiB arrive.

The size ceiling is what keeps a client that never sends a blank line from
holding this loop rather than the other way round.
-/
private partial def readHead (client : Socket.Client) (acc : ByteArray) : IO ByteArray := do
  if headComplete acc || acc.size > 16384 then
    return acc
  match ← Async.block (client.recv? 4096) with
  | none => return acc
  | some chunk => if chunk.isEmpty then return acc else readHead client (acc ++ chunk)

/--
Keeps reading so the client's body is drained rather than refused, which is
what a host that is merely slow looks like.

The loop stops once the listener is no longer wanted. After the far side has
closed there is nothing left to block on, so the wait between attempts is what
keeps a finished connection from spinning a processor until then.
-/
private partial def drain (listener : Listener) (client : Socket.Client) : IO Unit := do
  if !(← listener.running.get) then
    return
  match ← Async.block (client.recv? 65536) with
  | some chunk => if chunk.isEmpty then IO.sleep 50 else pure ()
  | none => IO.sleep 50
  drain listener client

/-- What one connection gets, once its head has been recorded. -/
private def follow (script : Script) (listener : Listener) (client : Socket.Client) : IO Unit := do
  let request ← readHead client ByteArray.empty
  listener.seen.modify (request :: ·)
  match script with
  | .blackHole =>
    listener.held.modify (client :: ·)
    drain listener client
  | .answer bytes =>
    try
      Async.block (client.send bytes)
      IO.sleep 200
    finally
      swallow (Async.block client.shutdown)
  | .redirect elsewhere =>
    let said :=
      "HTTP/1.1 302 Found\r\nLocation: " ++ elsewhere
        ++ "\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    try
      Async.block (client.send said.toUTF8)
    finally
      swallow (Async.block client.shutdown)

/--
Accepts connections until the listener is no longer wanted.

A connection that arrives after the action has ended is the knock that ends
this loop rather than a connection a property should hear about, so the count
is raised only once the listener is known to still be wanted.
-/
private partial def gather (script : Script) (listener : Listener) : IO Unit := do
  let client ← Async.block listener.gate.accept
  if !(← listener.running.get) then
    return
  listener.arrived.modify (· + 1)
  let _ ← IO.asTask (swallow (follow script listener client)) Task.Priority.dedicated
  gather script listener

/--
Connects once and says nothing, to release an accept that is already blocked.

Haskell closed the gate and let the blocked `accept` throw. This toolchain
offers no way to close a listening socket, so the loop is woken from the
outside instead, and the flag it then reads is what ends it.
-/
private def knock (port : UInt16) : IO Unit := do
  let client ← Socket.Client.mk
  Async.block do
    client.connect (loopback port)
    client.shutdown

/-- Runs a scripted listener for the duration of an action. -/
def withListener (script : Script) (act : Listener → IO α) : IO α := do
  let gate ← Socket.Server.mk
  gate.bind (loopback 0)
  gate.listen 64
  let taken ← gate.getSockName
  let listener : Listener := {
    gate
    port := taken.port
    arrived := ← IO.mkRef 0
    seen := ← IO.mkRef []
    held := ← IO.mkRef []
    running := ← IO.mkRef true }
  -- Swallowed, because the knock below leaves the loop with nothing further to
  -- accept, and whatever the last accept reports is the loop ending rather
  -- than a finding.
  let _ ← IO.asTask (swallow (gather script listener)) Task.Priority.dedicated
  try
    act listener
  finally
    -- The flag falls before the knock, so the woken accept reads a listener
    -- nobody wants and stops. Connections a black hole is holding are released
    -- last, which is what lets a client waiting on one give up.
    listener.running.set false
    swallow (knock listener.port)
    for client in ← listener.held.get do
      -- Cancelling the read is the release: shutting down only the write side
      -- would leave the drain loop waiting on a client that never speaks again.
      swallow client.native.cancelRecv
      swallow (Async.block client.shutdown)
    listener.held.set []

end Kusanagi.Listener
