/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Async.TCP
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Service
import Kusanagi.Stage

/-!
# Somebody with no address, talking raw HTTP to a real host

The scanner in `ARCHITECTURE.md` §3 holds no address and is building a list.
What it gets from a box is one answer, whatever it asks, and the properties
here ask everything a scanner would: the root, the usual well-known paths,
malformed addresses, methods the box does not implement, and objects of the
wrong size or at paths that climb out of the directory. None of it is allowed
to change the host's disk, and all of it must be answered with the same bytes.
-/

namespace Kusanagi.Scanner

open Std.Net
open Std.Async (Async)
open Std.Async.TCP
open Kusanagi.Door
open Kusanagi.Ground

/-- A key as a host files one: a period, a ward and an address. -/
def address : String := "0000000000000001/00ab/0123456789abcdef0123456789abcdef01234567"

private def get (path : String) : ByteArray :=
  ("GET " ++ path ++ " HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").toUTF8

private def put (path : String) (body : ByteArray) : ByteArray :=
  ("PUT " ++ path ++ " HTTP/1.1\r\nHost: x\r\nIf-None-Match: *\r\nContent-Length: "
    ++ toString body.size ++ "\r\nConnection: close\r\n\r\n").toUTF8 ++ body

/-- The same bytes over and over, which is the only body shape any of this needs. -/
private def filled (count : Nat) (byte : UInt8) : ByteArray :=
  ByteArray.mk (Array.replicate count byte)

/-- Enough of an answer to name it in a failure, without reprinting a body. -/
private def shown (bytes : ByteArray) (count : Nat) : String :=
  let slice := bytes.extract 0 (min count bytes.size)
  (String.fromUTF8? slice).getD s!"{slice.toList}"

/-- Whether the status line of an answer said 200. -/
private def answered200 (bytes : ByteArray) : Bool :=
  Stage.contains "200".toUTF8 (bytes.extract 0 (min 20 bytes.size))

/-- The same bytes with every ASCII capital folded down, so a search can ignore case. -/
private def lowered (bytes : ByteArray) : ByteArray :=
  ByteArray.mk (bytes.data.map fun byte => if byte ≥ 65 && byte ≤ 90 then byte + 32 else byte)

/-- Whether a haystack ends with a needle, byte for byte. -/
private def endsWith (needle haystack : ByteArray) : Bool := Id.run do
  if needle.size > haystack.size then
    return false
  let offset := haystack.size - needle.size
  for index in [0 : needle.size] do
    if needle[index]? != haystack[offset + index]? then
      return false
  return true

/--
The answers that differ from one another, in the order they first arrived.

Byte arrays carry no order this toolchain can sort on, so distinctness is
decided by comparing each answer against the ones already kept.
-/
private def distinct (answers : List ByteArray) : List ByteArray :=
  answers.foldl
    (fun kept answer =>
      if kept.any (fun other => other.toList == answer.toList) then kept else kept ++ [answer])
    []

/--
Where the host is listening.

The address the host announced is split at its last colon, and only a numeric
literal is accepted: the host says a numeric address it is already listening
on, so consulting a name service would only add a way to fail.
-/
private def resolve (host : String) : IO SocketAddress := do
  let refuse : IO SocketAddress :=
    throw <| IO.userError s!"nothing resolves {host}"
  let pieces := host.splitOn ":"
  let some spoken := pieces.getLast? | refuse
  let name := String.intercalate ":" pieces.dropLast
  let some port := spoken.toNat? | refuse
  if port > 65535 then refuse else
  match IPv4Addr.ofString name with
  | some addr => return .v4 { addr, port := port.toUInt16 }
  | none =>
    let bare := String.ofList ((name.toList.dropWhile (· == '[')).filter (· != ']'))
    match IPv6Addr.ofString bare with
    | some addr => return .v6 { addr, port := port.toUInt16 }
    | none => refuse

private def swallow (act : IO Unit) : IO Unit := try act catch _ => pure ()

/-- Reads until the far side stops talking, or the read is cancelled under it. -/
private partial def drain (client : Socket.Client) (acc : ByteArray) : IO ByteArray := do
  match ← try Async.block (client.recv? 65536) catch _ => pure none with
  | none => return acc
  | some chunk => if chunk.isEmpty then return acc else drain client (acc ++ chunk)

/--
Gives up on an answer after ten seconds.

This toolchain has no timeout combinator, so the deadline is a second task that
cancels the read the first one is sitting on. It wakes often enough that an
exchange which finished early is not charged for the whole ten seconds.
-/
private def deadline (client : Socket.Client) (finished expired : IO.Ref Bool) : IO Unit := do
  for _ in [0 : 400] do
    if ← finished.get then
      return
    IO.sleep 25
  unless ← finished.get do
    expired.set true
    swallow client.native.cancelRecv

/-- One request, one connection, the whole answer. -/
def exchange (host : String) (request : ByteArray) : IO ByteArray := do
  let there ← resolve host
  let client ← Socket.Client.mk
  Async.block (client.connect there)
  Async.block (client.send request)
  let finished ← IO.mkRef false
  let expired ← IO.mkRef false
  let watching ← IO.asTask (deadline client finished expired) Task.Priority.dedicated
  let answer ← drain client ByteArray.empty
  finished.set true
  let _ ← IO.wait watching
  swallow (Async.block client.shutdown)
  if ← expired.get then
    return "no answer within ten seconds".toUTF8
  else
    return answer

/-- Every file the host directory holds, by path. -/
private def objects (ground : Ground) : IO (List System.FilePath) := do
  return (← Stage.siteBytes ground.waypoint).map (·.1)

/--
What one directory holds, by name and one level deep.

Sorted, because a property about a directory changing must not depend on the
order the directory happened to be walked in.
-/
private def listing (directory : System.FilePath) : IO (List String) := do
  let found ← directory.readDir
  return (found.toList.map (·.fileName)).mergeSort (· ≤ ·)

/-- Every question a scanner asks gets one answer, byte for byte. -/
def everyStrangerGetsTheSameAnswer (door : Door) (ground : Ground) :
    IO (Except String Unit) :=
  Service.hosting door ground.waypoint fun host => do
    let upperHex : String := address.map fun letter =>
      if letter ≥ 'a' && letter ≤ 'f' then Char.ofNat (letter.toNat - 32) else letter
    let strangers : List (String × ByteArray) :=
      [ ("the root", get "/")
      , ("robots.txt", get "/robots.txt")
      , ("a well-known path", get "/.well-known/security.txt")
      , ("the drop prefix alone", get "/d/")
      , ("a short address", get "/d/zz")
      , ("an uppercase address", get ("/d/" ++ upperHex))
      , ("a 41-digit address", get ("/d/" ++ address ++ "0"))
      , ("an address nobody wrote", get ("/d/" ++ address))
      , ("OPTIONS", "OPTIONS / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n".toUTF8)
      , ("POST to a drop",
          ("POST /d/" ++ address
            ++ " HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").toUTF8)
      , ("DELETE of a drop",
          ("DELETE /d/" ++ address ++ " HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").toUTF8)
      , ("HEAD of the root", "HEAD / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n".toUTF8) ]
    let answers ← strangers.mapM fun (what, request) => do
      return (what, ← exchange host request)
    let apart := distinct (answers.map (·.2))
    let telling := answers.filterMap fun (what, answer) =>
      let folded := lowered answer
      if Stage.contains "kusanagi".toUTF8 folded || Stage.contains "server:".toUTF8 folded then
        some what
      else
        none
    if apart.length != 1 then
      let recital := String.join (answers.map fun (what, answer) =>
        what ++ ": " ++ shown answer 200 ++ "\n")
      return .error s!"a scanner gets {apart.length} different answers:\n{recital}"
    match telling.head? with
    | some what => return .error s!"the answer to {what} names a server or the project"
    | none => return .ok ()

/-- Objects of any size but the one size are never stored. -/
def aWrongSizeIsNeverStored (door : Door) (ground : Ground) : IO (Except String Unit) :=
  Service.hosting door ground.waypoint fun host => do
    let before ← objects ground
    for size in [0, 1, 131071, 131073, 200000] do
      let _ ← exchange host (put ("/d/" ++ address) (filled size 0x41))
    let after ← objects ground
    let back ← exchange host (get ("/d/" ++ address))
    if after != before then
      return .error <|
        s!"a wrong-sized object was stored: {after.length - before.length} new file(s)"
    if answered200 back then
      return .error "a wrong-sized object reads back"
    return .ok ()

/-- The first write at an address stands; the second changes nothing. -/
def anAddressIsWrittenOnce (door : Door) (ground : Ground) : IO (Except String Unit) :=
  Service.hosting door ground.waypoint fun host => do
    let first := filled 131072 0x41
    let second := filled 131072 0x42
    let one ← exchange host (put ("/d/" ++ address) first)
    let two ← exchange host (put ("/d/" ++ address) second)
    let back ← exchange host (get ("/d/" ++ address))
    if endsWith first back then
      return .ok ()
    else
      return .error <|
        s!"the second write at an address won, or the first was never stored: {shown back 60}"
          ++ s!"\n  first put: {shown one 60}\n  second put: {shown two 60}"

/-- Paths that climb out of the directory reach nothing and create nothing. -/
def traversalTouchesNothing (door : Door) (ground : Ground) : IO (Except String Unit) :=
  Service.hosting door ground.waypoint fun host => do
    let climbing : List String :=
      [ "/d/../../escaped"
      , "/d/..%2f..%2fescaped"
      , "/d/%2e%2e/%2e%2e/escaped"
      , "/../escaped"
      , "/d/" ++ address.take 38 ++ "/.."
      , "/d/" ++ address ++ "%00" ]
    let some parent := ground.waypoint.parent
      | return .error "the host directory has no parent to watch"
    let outsideBefore ← listing parent
    let before ← objects ground
    let answers ← climbing.mapM fun path =>
      exchange host (put path (filled 131072 0x43))
    let fetched ← climbing.mapM fun path => exchange host (get path)
    let outsideAfter ← listing parent
    let after ← objects ground
    if outsideAfter != outsideBefore then
      return .error s!"a traversal created something outside the host: {outsideAfter}"
    if after != before then
      return .error "a traversal created an object inside the host"
    match ((answers ++ fetched).filter answered200).head? with
    | some answer => return .error s!"a traversal was answered 200: {shown answer 80}"
    | none => return .ok ()

end Kusanagi.Scanner
