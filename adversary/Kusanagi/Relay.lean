/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Async.TCP
import Kusanagi.Service

/-!
# The person standing in front of the host, who sees packets and not bytes

Every other property here takes the host's position: it holds the objects, so it
can weigh them, count them and compare them. This module takes the position of
whoever carries the traffic — a network, a proxy, an employer, an internet
provider. That observer never sees a plaintext and never sees a key. It sees
**when a packet went past, and which way**.

**The host itself must not be the one keeping this record.**
`ARCHITECTURE.md` §3 line 0 says a host learns nothing, so a host that wrote down
request times would be a host that had learned something. The record is kept out
here instead, by a relay that forwards every byte unchanged and adds one line to
a list on the way past. The product is not modified, not configured and not aware
of it, which is the only way this measurement is evidence about the product
rather than about a debug mode.
-/

namespace Kusanagi.Relay

open Std.Net
open Std.Async (Async)
open Std.Async.TCP
open Kusanagi.Door

/--
One request going past, as much of it as a carrier can see.

The time is monotonic, so that a clock correction in the middle of a run cannot
produce a negative interval and a feature built on nonsense. It is counted in
nanoseconds rather than seconds, because `IO.monoNanosNow` is the monotonic
reading this toolchain offers; anything compared against a threshold scales.
-/
structure Observation where
  observedAt : Nat
  observedMethod : String
  observedPath : String
  deriving DecidableEq, Repr, Inhabited

instance : BEq Observation := ⟨fun left right => decide (left = right)⟩

instance : ToString Observation where
  toString seen := s!"{seen.observedAt} {seen.observedMethod} {seen.observedPath}"

/--
A host, and the wire everything to it goes down.

The gate is held in the structure because this toolchain's sockets have no
`close`: a socket lives as long as something refers to it, so the field is what
keeps the gate open for the length of the action.
-/
structure Relay where
  /-- The gate the accept loop is sitting on. -/
  private gate : Socket.Server
  /-- The port the operating system assigned, which nothing here chose. -/
  private port : UInt16
  /-- Where the real host is, already resolved. -/
  private upstream : SocketAddress
  private seen : IO.Ref (List Observation)
  /--
  Every socket this relay opened, in both directions, which teardown releases.

  A socket here has no `close`, so releasing one means cancelling the read it is
  waiting on; a connection still reading its head waits the same way a finished
  one does, and both are in this list.
  -/
  private taken : IO.Ref (List Socket.Client)
  /-- What each accepted connection is doing, so teardown can wait for it. -/
  private working : IO.Ref (List (Task (Except IO.Error Unit)))
  private running : IO.Ref Bool

/-- Where an endpoint is pointed so that its traffic passes this observer. -/
def Relay.locator (relay : Relay) : String := s!"http://127.0.0.1:{relay.port}"

/-- Everything that went past, oldest first. -/
def Relay.observed (relay : Relay) : IO (List Observation) := do
  return (← relay.seen.get).reverse

/--
A socket that is being torn down from the other side is not a finding.

Both ends of a relayed connection close, and whichever task loses that race gets
an error from a socket the other one has already released. That is the normal
end of a connection rather than a fault, and letting it escape would print a
stack trace into a test run that had gone perfectly.
-/
private def swallow (act : IO Unit) : IO Unit := try act catch _ => pure ()

/-- The loopback address at a port, which is the only place this ever listens. -/
private def loopback (port : UInt16) : SocketAddress :=
  .v4 { addr := IPv4Addr.ofParts 127 0 0 1, port }

/--
Where the real host is.

The address is split at its last colon so that a bracketed IPv6 literal keeps its
own colons, and the two literal forms are tried in turn. Nothing here consults a
name service: the host announces a numeric address it is already listening on, so
a lookup would only add a way to fail.
-/
private def resolve (upstream : String) : IO SocketAddress := do
  let refuse : IO SocketAddress :=
    throw <| IO.userError s!"the host announced an address nothing resolves: {upstream}"
  let pieces := upstream.splitOn ":"
  let some spoken := pieces.getLast? | refuse
  let host := String.intercalate ":" pieces.dropLast
  let some port := spoken.toNat? | refuse
  if port > 65535 then refuse else
  match IPv4Addr.ofString host with
  | some addr => return .v4 { addr, port := port.toUInt16 }
  | none =>
    let bare := String.ofList (host.toList.filter fun bracket => bracket != '[' && bracket != ']')
    match IPv6Addr.ofString bare with
    | some addr => return .v6 { addr, port := port.toUInt16 }
    | none => refuse

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

Bounded, because a request head that never ends is a request this relay refuses
to buffer rather than one it waits out.
-/
private partial def head (client : Socket.Client) (acc : ByteArray) : IO ByteArray := do
  if headComplete acc || acc.size > 16384 then
    return acc
  match ← Async.block (client.recv? 4096) with
  | none => return acc
  | some chunk => if chunk.isEmpty then return acc else head client (acc ++ chunk)

/-- Copies one direction until it ends. -/
private partial def pour (source sink : Socket.Client) : IO Unit := do
  match ← Async.block (source.recv? 65536) with
  | none => return
  | some chunk =>
    if chunk.isEmpty then
      return
    else
      Async.block (sink.send chunk)
      pour source sink

/--
Bytes decoded as text, with whatever is not UTF-8 replaced rather than refused.

A method and a path are ASCII in every request this relay will carry, so this
matters only for a client that sent something else: the record of what went past
is worth more than the decoding being exact.
-/
private def lenient (bytes : ByteArray) : String :=
  match String.fromUTF8? bytes with
  | some text => text
  | none =>
    String.ofList <| bytes.toList.map fun byte =>
      if byte < 128 then Char.ofNat byte.toNat else '\uFFFD'

/-- Records the first line, and nothing else. -/
private def note (relay : Relay) (at? : Nat) (request : ByteArray) : IO Unit := do
  let opening := lenient ⟨(request.toList.takeWhile (· != 13)).toArray⟩
  let words := ((opening.split Char.isWhitespace).toList.map (·.toString)).filter (!·.isEmpty)
  match words with
  | method :: path :: _ =>
    relay.seen.modify
      ({ observedAt := at?, observedMethod := method, observedPath := path } :: ·)
  | _ => pure ()

/--
Waits for the other direction to finish, but not for longer than this.

Bounded, because a client that never closes must not hold a test open. The
polling interval is what keeps the wait from costing a processor while the far
side is still talking.
-/
private def settle (working : Task (Except IO.Error Unit)) : Nat → IO Unit
  | 0 => pure ()
  | remaining + 1 => do
    if ← IO.hasFinished working then
      return
    IO.sleep 50
    settle working remaining

/--
Everything one connection carries, and the one line it leaves behind.

The box closes the connection after answering, so one connection is one request
and the head arrives before anything else. Recording it here rather than counting
bytes is deliberate: a carrier reading a TLS stream would see neither method nor
path, and the point of writing them down is to have a counterexample somebody can
read, not to claim the carrier has them.
-/
private def carry (relay : Relay) (client : Socket.Client) : IO Unit := do
  let server ← Socket.Client.mk
  relay.taken.modify (server :: ·)
  Async.block (server.connect relay.upstream)
  let request ← head client ByteArray.empty
  let at? ← IO.monoNanosNow
  note relay at? request
  Async.block (server.send request)
  -- The half-close dance, and it is not ceremony. A send returns when the
  -- operating system has taken the bytes, not when the far end has read them, so
  -- a relay that closed as soon as one direction ended would throw away a
  -- response it had already accepted — which an endpoint reports as
  -- `waypoint.io`, a fault in this instrument dressed as a fault in the host.
  let asked ← IO.asTask
    (do swallow (pour client server); swallow (Async.block server.shutdown))
    Task.Priority.dedicated
  swallow (pour server client)
  swallow (Async.block client.shutdown)
  settle asked 100

/--
Accepts connections until the relay is no longer wanted, and carries each one
without holding up the next.

A connection that arrives after the action has ended is the knock that ends this
loop rather than traffic a property should hear about, so nothing is recorded
until the relay is known to still be wanted.
-/
private partial def gather (relay : Relay) : IO Unit := do
  let client ← Async.block relay.gate.accept
  if !(← relay.running.get) then
    return
  relay.taken.modify (client :: ·)
  let working ← IO.asTask (swallow (carry relay client)) Task.Priority.dedicated
  relay.working.modify (working :: ·)
  gather relay

/--
Connects once and says nothing, to release an accept that is already blocked.

This toolchain offers no way to close a listening socket, so a blocked `accept`
cannot be made to throw. The loop is woken from the outside instead, and the flag
it then reads is what ends it.
-/
private def knock (port : UInt16) : IO Unit := do
  let client ← Socket.Client.mk
  Async.block do
    client.connect (loopback port)
    client.shutdown

/--
Runs a real host behind a relay for the duration of an action.

The directory is the one the host serves, which is the same directory
`Kusanagi.Ground` reads afterwards: what a host holds does not depend on how an
endpoint reached it, so one world answers both kinds of question.
-/
def withRelay (door : Door) (directory : System.FilePath) (act : Relay → IO α) : IO α :=
  Kusanagi.Service.hosting door directory fun announced => do
    let upstream ← resolve announced
    let gate ← Socket.Server.mk
    gate.bind (loopback 0)
    gate.listen 64
    let bound ← gate.getSockName
    let relay : Relay := {
      gate
      port := bound.port
      upstream
      seen := ← IO.mkRef []
      taken := ← IO.mkRef []
      working := ← IO.mkRef []
      running := ← IO.mkRef true }
    -- Swallowed, because the knock below leaves the loop with nothing further to
    -- accept, and whatever the last accept reports is the loop ending rather
    -- than a finding.
    let _ ← IO.asTask (swallow (gather relay)) Task.Priority.dedicated
    try
      act relay
    finally
      -- **The flag falls before the knock, and the order is the whole of it.**
      -- The woken accept reads a relay nobody wants and stops; a knock sent
      -- first would be carried to the host as if it were traffic. Sockets still
      -- waiting on a peer are released after that, which is what lets a
      -- half-finished connection give up.
      relay.running.set false
      swallow (knock relay.port)
      for client in ← relay.taken.get do
        swallow client.native.cancelRecv
        swallow (Async.block client.shutdown)
      -- Waiting is what makes the end of the action the end of the relay. A
      -- connection still carrying bytes after that would outlive the property
      -- that asked for it, and on this runtime it holds the whole process open.
      for answering in ← relay.working.get do
        let _ ← IO.wait answering

end Kusanagi.Relay
