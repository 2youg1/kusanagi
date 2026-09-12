/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Lean.Data.Json
import Kusanagi.Answer
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Stage
import Kusanagi.Veil

/-!
# What a member of a group holds, and what an ex-peer is still sent

A small group here is one endpoint's private roster and one drop per member,
so the promise to each member is that the others do not exist as far as their
own disk, their own reads and the host's objects can show. Revocation is the
other edge of the same promise: a member cut off must stop receiving, not
merely stop being read, or a fan-out keeps leaking to the person it was meant
to exclude.
-/

namespace Kusanagi.Insider

open Lean (Json)
open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage
open Kusanagi.Veil (apart)

/-- Alice, a roster of Bob and Mallory, and one sentence to both. -/
structure Team where
  withBob : Talk
  withMallory : Talk
  team : ChannelName
  landed : List Landed

private def assemble (door : Door) (ground : Ground) : IO Team := do
  let bob ← talk door ground .alice .bob (fresh "with-bob-only")
  let mallory ← talk door ground .alice .mallory (fresh "with-mallory-only")
  let name := fresh "the-whole-team"
  match ← Door.ask door bob.writer (.group name [bob.channel, mallory.channel]) with
  | .accepted (.grouped ..) => pure ()
  | other => throw <| IO.userError s!"the group was refused: {repr other}"
  let fanned ← Door.ask door bob.writer (.sendGroup name "the quarterly numbers, to everybody")
  -- Reading once is how the inviter learns who accepted; revoking needs that.
  for stage in [bob, mallory] do
    let _ ← hear door stage.writer stage.channel
    pure ()
  match fanned with
  | .accepted (.fannedOut _ delivered) =>
    return { withBob := bob, withMallory := mallory, team := name, landed := delivered }
  | other => throw <| IO.userError s!"the fan-out was refused: {repr other}"

/-- A needle as a failure can name it, and its bytes when they are not text. -/
private def spelled (bytes : ByteArray) : String :=
  (String.fromUTF8? bytes).getD (toString bytes.toList)

/--
Bob hands over his disk and everything he can read: Mallory is in none of it.
-/
def aMemberLearnsNoOtherMember (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let squad ← assemble door ground
  let bob := squad.withBob.reader
  let channel := squad.withBob.channel.said
  let mallorysChannel := squad.withMallory.channel.said
  let groupName := squad.team.said
  let reading ← Door.typed door ["--root", bob.toString, "--json", "read", "--from", "-"]
    (some (channel ++ "\n").toUTF8)
  let listing ← Door.typed door ["--root", bob.toString, "--json", "channels"] none
  let disk ← siteBytes bob
  let names ← siteNames bob
  let held ← ground.stored
  let needles := handleOf squad.withMallory.readerHandle ++
    [mallorysChannel, groupName].map String.toUTF8
  let haystacks : List (System.FilePath × ByteArray) :=
    disk
      ++ [(⟨"read"⟩, reading.out), (⟨"channels"⟩, listing.out)]
      ++ names.map (fun name => (⟨"a file name"⟩, name.toUTF8))
      ++ held.map (fun (_, bytes) => (⟨"host object"⟩, bytes))
  let found := needles.flatMap fun needle =>
    (anyContains needle haystacks).map fun place => (needle, place)
  match found with
  | [] => return .ok ()
  | (needle, place) :: _ =>
    return .error s!"a member can find {spelled needle} in {place}"

/-- The keys of every JSON object in the `segments` array of one read, sorted. -/
private def segmentKeys (raw : ByteArray) : Option (List (List String)) := do
  let text ← String.fromUTF8? raw
  let parsed ← (Json.parse text).toOption
  let segments ← (parsed.getObjVal? "segments").toOption
  let listed ← (segments.getArr?).toOption
  return listed.toList.filterMap fun segment =>
    match segment with
    | .obj fields => some (fields.keys.mergeSort (· ≤ ·))
    | _ => none

/-- A segment that was fanned out has the same JSON keys as one that was not. -/
def aBroadcastLooksLikeAWhisper (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let squad ← assemble door ground
  let bob := squad.withBob.reader
  let channel := squad.withBob.channel.said
  let _ ← say door squad.withBob.writer squad.withBob.channel "to bob alone"
  let reading ← Door.typed door ["--root", bob.toString, "--json", "read", "--from", "-"]
    (some (channel ++ "\n").toUTF8)
  return match segmentKeys reading.out with
    | none => .error "the read did not parse as an object with segments"
    | some [broadcast, whisper] =>
      if broadcast == whisper then .ok ()
      else .error s!"a broadcast carries keys {broadcast} and a whisper {whisper}"
    | some other => .error s!"expected two segments, saw {other.length}"

/-- The two drops of one sentence share nothing on the host. -/
def aBroadcastIsTwoStrangers (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let squad ← assemble door ground
  let held ← ground.stored
  let addresses := squad.landed.filterMap fun where' =>
    if where'.status == "sent" then where'.address else none
  let bodies := (held.filter fun (address, _) => addresses.contains address).map (·.2)
  return match bodies with
    | [left, right] =>
      match apart left right with
      | none => .ok ()
      | some why => .error why
    | _ =>
      .error
        s!"the fan-out named {addresses.length} addresses and the host holds \
           {bodies.length} of them"

/-- What one member's copy of a fan-out was reported to have done. -/
private def copyFor (delivered : List Landed) (member : ChannelName) : List Landed :=
  delivered.filter (·.member == member)

/--
After Bob is revoked, a fan-out lands on Mallory and not on Bob, says so with a
code, and Bob's next read does not carry the sentence.
-/
def aRevokedMemberIsLeftOut (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let squad ← assemble door ground
  let alice := squad.withBob.writer
  let revoked ← Door.ask door alice (.revoke squad.withBob.channel)
  let fanned ← Door.ask door alice (.sendGroup squad.team "after bob was cut off")
  let bobHears ← hear door squad.withBob.reader squad.withBob.channel
  let malloryHears ← hear door squad.withMallory.reader squad.withMallory.channel
  let sentence := "after bob was cut off"
  return do
    match revoked with
    | .accepted (.revoked ..) => .ok ()
    | other => .error s!"revoke answered {repr other}"
    match fanned with
    | .accepted (.fannedOut _ delivered) => do
      let cut := copyFor delivered squad.withBob.channel
      match cut with
      | [only] =>
        if only.status != "sent" && only.code.isSome then .ok ()
        else .error s!"the fan-out reported the revoked member as {repr cut}"
      | _ => .error s!"the fan-out reported the revoked member as {repr cut}"
      let kept := copyFor delivered squad.withMallory.channel
      match kept with
      | [only] =>
        if only.status == "sent" then .ok ()
        else .error s!"the fan-out reported the remaining member as {repr kept}"
      | _ => .error s!"the fan-out reported the remaining member as {repr kept}"
    | .refused complaint =>
      .error s!"a fan-out with one revoked member was refused outright: {repr complaint}"
    | .accepted other => .error s!"the fan-out answered {repr other}"
    match entriesOf bobHears with
    | .ok entries =>
      if !((entries.map (·.carried.shown)).contains sentence) then .ok ()
      else .error "the revoked member received the fan-out"
    | .error _ => .ok ()
    match entriesOf malloryHears with
    | .ok entries =>
      if (entries.map (·.carried.shown)).contains sentence then .ok ()
      else .error s!"the remaining member did not receive the fan-out: {repr entries}"
    | .error why => .error s!"the remaining member did not receive the fan-out: {why}"

/--
Sending on a channel whose peer was revoked fails with the same code as reading
one, so a revocation is one fact and not a reading-only fact.
-/
def sendingToTheRevokedFailsLikeReadingThem (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "cut-off-both-ways")
  let _ ← say door stage.reader stage.channel "before"
  let _ ← hear door stage.writer stage.channel
  let revoked ← Door.ask door stage.writer (.revoke stage.channel)
  let reading ← hear door stage.writer stage.channel
  let sending ← Door.ask door stage.writer (.send stage.channel "after")
  return match revoked, reading, sending with
    | .refused complaint, _, _ => .error s!"the inviter could not revoke: {repr complaint}"
    | .accepted _, .refused reader, .refused writer =>
      if reader.code == writer.code then .ok ()
      else
        .error
          s!"reading a revoked peer fails with {reader.code} and sending to one with \
             {writer.code}"
    | .accepted _, .refused _, .accepted outcome =>
      .error s!"a revoked peer can still be sent to: {repr outcome}"
    | .accepted _, other, _ => .error s!"reading a revoked peer answered {repr other}"

end Kusanagi.Insider
