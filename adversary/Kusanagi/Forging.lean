/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Stage

/-!
# Everything else a host can do to the bytes it holds

`Kusanagi.Lying` covers a moved object and a vanished one. This module takes
the rest of the host's power over its own disk: any byte, any shape, any
position, any object copied from anywhere, extra objects, and the one-time
address reset that lets a second person accept a spent invitation. Every
property is a relation between a reader's answer before the lie and after
it, and the shape of every acceptable answer is the same: a coded refusal,
or a height no greater than what was honestly verifiable, and never a
segment its author did not put there.
-/

namespace Kusanagi.Forging

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/-- The first verdict that is not `held`, which is how a list of findings is settled. -/
private def allOf (findings : List Verdict) : Verdict :=
  findings.foldl (fun found next => match found with | .held => next | other => other) .held

/-- The answer never carries this payload as a segment, whatever else it does. -/
private def neverCarried (what : String) (carried : Carried) (answer : Answer) : Verdict :=
  match answer with
  | .refused _ => .held
  | .accepted (.read _ _ _ entries) =>
    if entries.any fun entry => decide (entry.carried = carried) then
      .broke s!"{what} was read as a segment carrying {(toString (repr carried)).take 80}"
    else
      .held
  | .accepted other => .broke s!"{what} made a read answer with {repr other}"

/-- The answer never carries this text as a segment, whatever else it does. -/
private def neverShown (what text : String) (answer : Answer) : Verdict :=
  neverCarried what (.asText text) answer

private def hexOfBytes (bytes : ByteArray) : String :=
  let digits := "0123456789abcdef".toList
  String.ofList <| bytes.toList.flatMap fun byte =>
    [(digits[(byte / 16).toNat]?).getD '0', (digits[(byte % 16).toNat]?).getD '0']

private def neverShownBytes (what : String) (bytes : ByteArray) (answer : Answer) : Verdict :=
  neverCarried what (.asBytes (hexOfBytes bytes)) answer

/-- A reader that reports a height at or above a gap has skipped the gap. -/
private def boundedBelow (who : String) (gap : UInt64) (answer : Answer) : Verdict :=
  match answer with
  | .refused _ => .held
  | .accepted (.read _ _ (some height) _) =>
    if height ≥ gap then .broke s!"{who} reported height {height} across a gap at {gap}"
    else .held
  | .accepted (.read ..) => .held
  | .accepted other => .broke s!"{who} answered with {repr other}"

/-- A reader that has verified a height is never talked below it. -/
private def atLeast (who : String) (floor : UInt64) (answer : Answer) : Verdict :=
  match answer with
  | .refused _ => .held
  | .accepted (.read _ _ height _) =>
    if height.any (· ≥ floor) then .held
    else .broke s!"{who} was talked down to {repr height}"
  | .accepted other => .broke s!"{who} answered with {repr other}"

/-- The peer a listing names for one channel. -/
private def peerListed (door : Door) (site : System.FilePath) (channel : ChannelName) :
    IO (Option String) := do
  match ← Door.ask door site .channels with
  | .accepted (.channels summaries) =>
    return ((summaries.filter fun listed => decide (listed.name = channel)).head?).bind (·.peer)
  | _ => return none

/--
The first byte, a middle byte and the last byte — the last being inside
the pad — each refuse the object when changed.
-/
def anyByteFlippedIsRefused (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "every-byte-is-checked")
  let address ← say door stage.writer stage.channel "intact"
  let original ← ground.holding address
  let size := original.size
  let mut findings : List Verdict := []
  for (which, offset) in [("first", 0), ("middle", size / 2), ("last", size - 1)] do
    ground.damage offset address
    let answer ← hear door stage.reader stage.channel
    ground.plant address original
    findings := findings ++ [neverShown s!"the {which} byte changed" "intact" answer]
  return allOf findings

/--
Truncated, extended, zeroed, emptied or replaced with noise of the right
size: none of them is ever reported as a segment, and a later honest
segment is not reported either, because the chain has a hole.
-/
def aWrongShapeIsNotASegment (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "shape-is-checked")
  let first ← say door stage.writer stage.channel "well formed"
  let original ← ground.holding first
  let size := original.size
  let wheel : List UInt8 := [7, 91, 200, 13, 42, 255, 0, 128]
  let noise := ByteArray.mk ((List.range size).map fun index =>
    (wheel[index % wheel.length]?).getD 0).toArray
  let shapes : List (String × ByteArray) :=
    [ ("one byte short", original.extract 0 (size - 1))
    , ("one byte long", original ++ ByteArray.mk #[0])
    , ("all zero", ByteArray.mk (List.replicate size (0 : UInt8)).toArray)
    , ("empty", ByteArray.mk #[])
    , ("noise of the right size", noise) ]
  let mut findings : List Verdict := []
  for (which, bytes) in shapes do
    ground.plant first bytes
    let answer ← hear door stage.reader stage.channel
    ground.plant first original
    findings := findings ++ [neverShown which "well formed" answer]
  -- A squatted next address: whatever the host puts there, the author's next
  -- send is a coded refusal or lands beyond it, and the reader never sees junk.
  let _ ← hear door stage.reader stage.channel
  ground.vanish first
  let squatted := ByteArray.mk (List.replicate size (1 : UInt8)).toArray
  ground.plant first squatted
  let sent ← Door.ask door stage.writer (.send stage.channel "after the squat")
  let after ← hear door stage.reader stage.channel
  let squat : Verdict :=
    match sent with
    | .refused _ => .held
    | .accepted (.sent _ index _) =>
      if index > 0 then .held
      else .broke s!"a send over a squatted address reported {repr sent}"
    | .accepted other => .broke s!"a send over a squatted address reported {repr other}"
  return allOf (findings ++ [squat, neverShownBytes "a squatted address" squatted after])

/--
The peer's own genuine drop, served at the author's next address, is not
the author's segment: the key is derived from who wrote, not only where.
-/
def aPeersDropIsNotTheAuthors (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "authors-are-not-interchangeable")
  let alice ← ["alice one", "alice two"].mapM (say door stage.writer stage.channel)
  let bob ← say door stage.reader stage.channel "bob speaking"
  match alice with
  | [_, second] =>
    ground.vanish second
    ground.transplant bob second
    let answer ← hear door stage.reader stage.channel
    return neverShown "the peer's drop at the author's address" "bob speaking" answer
  | _ => return .broke "the author did not write two segments"

/--
A genuine drop from another channel, served at this channel's address,
opens under neither key.
-/
def anotherChannelsDropIsRefused (door : Door) (ground : Ground) : IO Verdict := do
  let here ← talk door ground .alice .bob (fresh "this-channel")
  let there ← talk door ground .alice .mallory (fresh "that-channel")
  let mine ← say door here.writer here.channel "for bob"
  let theirs ← say door there.writer there.channel "for mallory"
  ground.vanish mine
  ground.transplant theirs mine
  let answer ← hear door here.reader here.channel
  return neverShown "another channel's drop" "for mallory" answer

/--
A missing middle segment stops a reader with no memory below the gap, and
moves a reader with memory nowhere at all.
-/
def aGapStopsAFreshReaderAndMovesNobodyBack (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "no-gap-is-skipped")
  let addresses ← ["zero", "one", "two", "three"].mapM (say door stage.writer stage.channel)
  match addresses with
  | [_, second, _, _] =>
    let middle ← ground.holding second
    ground.vanish second
    let forgetful ← hear door stage.reader stage.channel
    ground.plant second middle
    let remembered ← hear door stage.reader stage.channel
    ground.vanish second
    let again ← hear door stage.reader stage.channel
    return allOf
      [ boundedBelow "a reader with no memory" 1 forgetful
      , atLeast "a reader that verified to three" 3 remembered
      , atLeast "a reader that verified to three, after the gap reopened" 3 again ]
  | _ => return .broke "the author did not write four segments"

/--
Two adjacent drops with their contents exchanged are refused, or the read
stops below the first of them.
-/
def swappedDropsAreRefused (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "order-is-in-the-key")
  let addresses ← ["first", "second"].mapM (say door stage.writer stage.channel)
  match addresses with
  | [one, two] =>
    let a ← ground.holding one
    let b ← ground.holding two
    ground.plant one b
    ground.plant two a
    let answer ← hear door stage.reader stage.channel
    return allOf
      [ neverShown "swapped drops" "first" answer
      , neverShown "swapped drops" "second" answer ]
  | _ => return .broke "the author did not write two segments"

private def hexDigit (digit : Nat) : Char :=
  ("0123456789abcdef".toList[digit % 16]?).getD '0'

private def junkName (n : Nat) : String :=
  String.ofList (List.replicate 2 (hexDigit (n / 16)) ++ List.replicate 36 (hexDigit (n % 16)))
    ++ "0" ++ String.singleton (hexDigit (n % 13))

/--
A hundred extra objects in the reader's own bin change nothing a reader
reports: the reader takes the bin whole and keeps what its addresses match.
-/
def junkChangesNothing (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "the-host-is-never-listed")
  let said ← ["one", "two", "three"].mapM (say door stage.writer stage.channel)
  let before ← hear door stage.reader stage.channel
  let bin :=
    match said with
    | first :: _ => binOf first
    | [] => ""
  for n in [0:100] do
    ground.plant ⟨bin ++ "/" ++ junkName n⟩
      (ByteArray.mk (List.replicate 131072 (UInt8.ofNat n)).toArray)
  let after ← hear door stage.reader stage.channel
  if decide (before = after) then
    return .held
  else
    return .broke
      s!"extra objects on the host changed a read:\n  before: {repr before}\n  after:  {repr after}"

/--
Once the inviter has seen who accepted, the host deleting that acceptance
and a second person accepting the same invitation changes nothing: the peer
stays who it was, and the impostor's segments never appear.
-/
def theFirstPeerIsPinned (door : Door) (ground : Ground) : IO Verdict := do
  let offered ← Door.ask door (ground.siteOf .alice)
    (.invite (fresh "one-acceptance-only") ground.waypoint .forever both)
  let (channel, invitation) ←
    match offered with
    | .accepted (.invited name line _) => pure (name, line)
    | other => throw <| IO.userError s!"the invitation was refused: {repr other}"
  let beforeJoin := (← ground.stored).map (·.1)
  let joined ← Door.ask door (ground.siteOf .bob) (.join invitation channel)
  let afterJoin := (← ground.stored).map (·.1)
  let pinned ← hear door (ground.siteOf .alice) channel
  let peerBefore ← peerListed door (ground.siteOf .alice) channel
  for address in afterJoin.filter (fun a => !beforeJoin.any fun b => decide (b = a)) do
    ground.vanish address
  let impostor ← Door.ask door (ground.siteOf .mallory) (.join invitation channel)
  let _ ← Door.ask door (ground.siteOf .mallory) (.send channel "impostor")
  let _ ← say door (ground.siteOf .bob) channel "still bob"
  let answer ← hear door (ground.siteOf .alice) channel
  let peerAfter ← peerListed door (ground.siteOf .alice) channel
  return allOf
    [ (match joined with
       | .accepted (.joined ..) => .held
       | other => .broke s!"the first acceptance was refused: {repr other}")
    , (match pinned with
       | .accepted (.read ..) => .held
       | other => .broke s!"the inviter could not read after the acceptance: {repr other}")
    , neverShown s!"a second acceptance ({repr impostor})" "impostor" answer
    , ensure (peerAfter == peerBefore)
        s!"the listed peer changed from {repr peerBefore} to {repr peerAfter}" ]

/--
On a releasing channel an acknowledged drop stays on the host — a reader
names no address, and a delete would (D-20) — and it opens for nobody:
neither reader reports it again, and neither disk holds it.
-/
def aReleasedDropIsGoneAndStaysGone (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talkWith door ground .alice .bob
    (.inviteReleasing (fresh "burn-after-reading") ground.waypoint)
  let address ← say door stage.writer stage.channel "burn me"
  let copy ← ground.holding address
  let _ ← hear door stage.reader stage.channel
  let _ ← say door stage.reader stage.channel "read it"
  let _ ← hear door stage.writer stage.channel
  let held := (← ground.stored).map (·.1)
  ground.plant address copy
  let reader ← hear door stage.reader stage.channel
  let writer ← hearMine door stage.writer stage.channel
  let disks := (← [stage.writer, stage.reader].mapM siteBytes).flatten
  return allOf
    [ ensure (held.any fun key => decide (key = address))
        "a release deleted a drop, which names an address to the host"
    , neverShown "a released drop put back" "burn me" reader
    , neverShown "a released drop put back, to its own author" "burn me" writer
    , (match anyContains "burn me".toUTF8 disks with
       | [] => .held
       | path :: _ => .broke s!"a released message is on disk at {path}") ]

/--
A host restored from a backup has the author's own stream shorter than
both ends remember. A reader with no memory of the stream is refused, and
the reader that remembers still receives the author's next segment: it
resumes from its own record, the new segment links to the head it verified,
and the one with a memory is right. What must not happen is the author
confirming its predecessor first — that names two adjacent addresses to the
host on every send, which is the stream's shape given away.
-/
def aRolledBackHostStopsTheAuthorToo (door : Door) (ground : Ground) : IO Verdict := do
  let stage ← talk door ground .alice .bob (fresh "restored-from-a-backup")
  let first ← say door stage.writer stage.channel "one"
  let backup ← ground.holding first
  let later ← ["two", "three"].mapM (say door stage.writer stage.channel)
  let _ ← hear door stage.reader stage.channel
  later.forM ground.vanish
  ground.plant first backup
  let author ← Door.ask door stage.writer (.send stage.channel "four, after the rollback")
  let forgetful ← hear door stage.reader stage.channel
  let remembering ← Door.ask door stage.reader (.readAfter stage.channel 2)
  let resumed :=
    match remembering with
    | .accepted (.read _ _ (some height) [entry]) =>
      height == 3 && entry.index == 3 &&
        decide (entry.carried = .asText "four, after the rollback")
    | _ => false
  return allOf
    [ (match author with
       | .accepted (.sent ..) => .held
       | other => .broke s!"after the rollback the author could not write: {repr other}")
    , (match forgetful with
       | .refused _ => .held
       | other =>
         .broke s!"a reader walking the whole stream did not notice the rollback: {repr other}")
    , ensure resumed
        s!"the reader that remembers did not receive the segment after the rollback: {repr remembering}" ]

end Kusanagi.Forging
