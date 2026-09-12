/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Cairn
import Kusanagi.Keyboard
import Kusanagi.Lying
import Kusanagi.Regression

/-!
# The hunts that quantify over traces rather than over one command

Each of these builds a world, runs something the model generated in it, and
throws the world away. The worlds are separate on purpose: two traces sharing a
host would interfere in ways that read as broken rules rather than as a harness
that was asked to do the impossible.
-/

namespace Kusanagi.Hunt

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Dynamic
open Kusanagi.Ground
open Kusanagi.Model

/--
Whether the host's view links anything to anything.

Three claims in one, over the addresses and over the bytes. An address is never
reused and no two of them look alike, so the host cannot group drops by where
they sit; and no two objects are byte-identical, so it cannot group them by what
they contain either. The third is the one that catches a suite of ciphertexts
that stopped being distinct — a key reused across two drops, or a nonce that
repeated — which is invisible from the address side.
-/
def unlinkable (held : List (Address × ByteArray)) : Verdict :=
  -- The address alone: the period and the ward before it are public and shared
  -- by every object in a bin, by design.
  let addressOf (key : String) : String := ((key.splitOn "/").getLast?).getD key
  -- Sorted, so the pair with the longest shared prefix is always adjacent.
  let sorted := (held.map (·.1.key)).mergeSort (· ≤ ·)
  let shared (left right : String) : Nat :=
    ((addressOf left).toList.zip (addressOf right).toList).takeWhile (fun p => p.1 == p.2) |>.length
  let apart := (sorted.zip (sorted.drop 1)).all fun (earlier, later) =>
    earlier != later && shared earlier later < 8
  let bodies := held.map (fun entry => entry.2.toList)
  let distinct := (bodies.mergeSort (fun a b => a.toString ≤ b.toString)).eraseDups.length
  if !apart then .broke "two of the host's addresses are alike, or one was reused"
  else ensure (distinct == bodies.length) "two objects the host holds are the same bytes"

/-- Any trace at all, plus what the host is left holding afterwards. -/
def traces (door : Door) (seed : StdGen) (runs : Nat := 20) : IO Verdict :=
  huntWith World (arbitraryActions World) runs seed fun actions =>
    withGround fun ground => do
      match ← runActions (Realized := Realized) World ({ door, ground } : Kit) actions with
      | .held => return unlinkable (← ground.stored)
      | other => return other

/-- Any prefix, one revocation, any suffix, and a read that must still fail. -/
def revocation (door : Door) (seed : StdGen) (runs : Nat := 10) : IO Verdict :=
  huntWith World (forAllScript World revocationIsFinal) runs seed fun actions =>
    withGround fun ground => do
      match ← runActions (Realized := Realized) World ({ door, ground } : Kit) actions with
      | .held => return unlinkable (← ground.stored)
      | other => return other

private def refused : Answer → Bool
  | .refused _ => true
  | _ => false

/--
The simplest lie a host can tell: one changed byte.

What the reader must never do is show it. Nothing in the command asks for a
check, which is the point — verification is not an option a caller can forget.
-/
def tampering (door : Door) : IO Verdict := withGround fun ground => do
  let one : ChannelName := ⟨"one"⟩
  let asking (site : Site) (verb : Verb) := Door.ask door (ground.siteOf site) verb
  match ← asking .alice (.invite one ground.waypoint .forever both) with
  | .accepted (.invited _ invitation _) =>
    match ← asking .bob (.join invitation one) with
    | .accepted (.joined ..) =>
      match ← asking .bob (.send one "a message that must arrive intact") with
      | .accepted (.sent _ _ address) =>
        match ← asking .alice (.read one) with
        | .accepted (.read _ _ _ [_]) =>
          ground.corrupt address
          return ensure (refused (← asking .alice (.read one)))
            "a changed byte was shown to the reader rather than refused"
        | other => return .broke s!"the segment did not arrive intact: {repr other}"
      | other => return .broke s!"nothing was written to corrupt: {repr other}"
    | other => return .broke s!"the invitation was not accepted: {repr other}"
  | other => return .broke s!"the invitation was refused: {repr other}"

/-- One throwaway world per lie, because each of them damages the host. -/
private def lyingWith (door : Door) (count : Nat)
    (act : Door → Ground → Lying.Written → IO Verdict) : IO Verdict :=
  withGround fun ground => do
    let written ← Lying.writeSome door (ground.siteOf .alice) (ground.siteOf .bob)
      ground.waypoint (2 + count % 4)
    act door ground written

/-- The host serves real bytes from the wrong place. -/
def transplanting (door : Door) (seed : StdGen) : IO Verdict :=
  forAll 4 (Gen.choose 0 7) toString
    (fun count => lyingWith door count Lying.transplantIsRefused) seed

/-- The host stops serving something a reader has already verified. -/
def vanishing (door : Door) (seed : StdGen) : IO Verdict :=
  forAll 4 (Gen.choose 0 7) toString
    (fun count => lyingWith door count Lying.historyNeverShrinks) seed

/--
Somebody types the line from the README with one finger in the wrong place.

Three things are asserted about whatever comes back, and none of them is an
expected output: it has one of the two shapes this door defines, every command
its advice names can be taken, and the advice is about something that was
actually supplied.
-/
def keyboard (door : Door) (seed : StdGen) : IO Verdict :=
  forAll 24 Keyboard.anyChoice (fun _ => "a mistyped command line")
    (fun choice => withGround fun ground => do
      let bench ← Keyboard.prepare door (ground.siteOf .alice) (ground.siteOf .bob)
        ground.waypoint
      let typing := Keyboard.typingOf bench choice
      let meant := " ".intercalate typing.intended
      let keyed := " ".intercalate typing.keyed
      let said := s!"\n  meant:  kusanagi {meant}\n  typed:  kusanagi {keyed}\n  slip:   {typing.slipped.name}"
      match ← Keyboard.shapeIsAnswerable door typing.keyed with
      | .error reason => return .broke (reason ++ said)
      | .ok (.accepted _) => return .held
      | .ok (.refused complaint) =>
        match ← Keyboard.adviceIsExecutable door bench.site complaint with
        | .error reason => return .broke (reason ++ said)
        | .ok () =>
          match Keyboard.adviceIsAboutWhatWasGiven typing.keyed complaint with
          | .error reason => return .broke (reason ++ said)
          | .ok () => return .held) seed

/-- An agent sends bytes it did not choose the shape of. -/
def piping (door : Door) (seed : StdGen) : IO Verdict :=
  forAll 12 Gen.bytes (fun payload => s!"{payload.size} bytes")
    (fun payload => withGround fun ground => do
      let bench ← Keyboard.prepare door (ground.siteOf .alice) (ground.siteOf .bob)
        ground.waypoint
      match ← Keyboard.bytesSurviveTheTrip door bench.site bench.channel payload with
      | .error reason => return .broke reason
      | .ok () => return .held) seed

/--
A reader that remembers is still told everything.

An endpoint now writes down how far it has verified a stream, so that a poll
names one address to the host instead of every address of the conversation. The
way that fix goes wrong is by reading less than it reports, and no assertion
about a message arriving would notice. So these are relations: a floor hides
exactly the entries at or below it and nothing else, and a second read is not
paid for out of what the first one wrote down.
-/
def remembering (door : Door) (seed : StdGen) : IO Verdict :=
  forAll 8 (do return ((← Gen.choose 0 5), (← Gen.choose 0 7)))
    (fun (count, level) => s!"{count} said, floor {level}")
    (fun (count, level) => withGround fun ground => do
      let stocked ← Cairn.stock door (ground.siteOf .alice) (ground.siteOf .bob)
        ground.waypoint count
      let floor := UInt64.ofNat level
      let outcomes := [ ← Cairn.floorHidesExactlyWhatItNames door stocked floor
                      , ← Cairn.readingTwiceSubtractsNothing door stocked (some floor)
                      , ← Cairn.readingTwiceSubtractsNothing door stocked none ]
      let reasons := outcomes.filterMap fun outcome =>
        match outcome with
        | .error why => some why
        | .ok () => none
      return match reasons with
        | [] => .held
        | _ => .broke ("\n".intercalate reasons)) seed

end Kusanagi.Hunt
