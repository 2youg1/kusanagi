/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Door

/-!
# What a reader must still be told after it has started remembering

An endpoint now writes down how far it has verified a stream, so that the next
read resumes instead of naming every address of the stream to the host again.
That is a privacy fix, and the way a privacy fix of this shape goes wrong is by
quietly reading less than it reports: a resumed walk that started too high would
hand back a short list and a correct height, and every assertion about one
message arriving would still pass.

So both properties here are relations between two traces of the same run, and
neither names an expected output:

* a read that names a floor agrees, entry for entry, with a read that names
  none — with exactly the entries at or below the floor missing;
* reading twice reports what reading once reported, which is the assertion that
  the memory written by the first read cannot subtract from the second.

Neither reaches into a site. Whether the memory is a file, where it lives, and
what it is called are all things this module must not know, because the day it
knows them is the day it stops testing the door and starts testing the
implementation.
-/

namespace Kusanagi.Cairn

open Kusanagi.Answer
open Kusanagi.Door

/-- A channel with a known number of segments on the peer's stream. -/
structure Stocked where
  reader : System.FilePath
  channel : ChannelName
  height : UInt64
  deriving Repr, Inhabited

/-- The name both endpoints open the conversation under. Names are local. -/
private def peer : ChannelName := ⟨"peer"⟩

/-- Puts one segment on the writer's stream, and refuses to continue if it is refused. -/
private def say (door : Door) (writer : System.FilePath) (n : Nat) : IO Unit := do
  match ← Door.ask door writer (.send peer s!"segment {n}") with
  | .accepted (.sent ..) => pure ()
  | other => throw <| IO.userError s!"a segment was refused: {repr other}"

/--
Opens a channel and puts `count` segments on it, from writer to reader.

Nothing is read here. A reader that has never read has nothing written down, so
every property below starts from the state in which the fix is not yet doing
anything — and reaches the state in which it is by reading, which is what a
caller does too.
-/
def stock (door : Door) (writer reader host : System.FilePath) (count : Nat) : IO Stocked := do
  let invitation ←
    match ← Door.ask door writer (.invite peer host .forever both) with
    | .accepted (.invited _ line _) => pure line
    | other => throw <| IO.userError s!"the invitation was refused: {repr other}"
  match ← Door.ask door reader (.join invitation peer) with
  | .accepted (.joined ..) => pure ()
  | other => throw <| IO.userError s!"the channel could not be joined: {repr other}"
  for step in List.range count do
    say door writer (step + 1)
  return { reader, channel := peer, height := UInt64.ofNat count }

/--
One read, reduced to the two facts a caller acts on.

Entries are put in index order rather than trusted to arrive in it: this module
is asserting which entries came back, and letting an ordering bug masquerade as
a missing-segment bug would point the next reader at the wrong file.
-/
private def readingOf (door : Door) (stocked : Stocked) (level : Option UInt64) :
    IO (Except String (Option UInt64 × List Entry)) := do
  let verb :=
    match level with
    | none => Verb.read stocked.channel
    | some above => Verb.readAfter stocked.channel above
  match ← Door.ask door stocked.reader verb with
  | .accepted (.read _ _ height entries) =>
    return .ok (height, entries.mergeSort fun left right => left.index ≤ right.index)
  | other => return .error s!"{repr other}"

/-- The heights of a read, which is what a failure names rather than the payloads. -/
private def heights (entries : List Entry) : List UInt64 := entries.map Entry.index

/--
A floor hides the entries at or below it, and hides nothing else.

The height is compared too, and separately. A resumed read that lost the
stream's head would be a different bug with the same cause, and reporting the
right segments under the wrong height is worse than failing: an agent uses the
height to decide where to poll from next.
-/
def floorHidesExactlyWhatItNames (door : Door) (stocked : Stocked) (level : UInt64) :
    IO (Except String Unit) := do
  let whole ← readingOf door stocked none
  let above ← readingOf door stocked (some level)
  match whole, above with
  | .error reason, _ => return .error s!"reading the whole stream failed: {reason}"
  | _, .error reason => return .error s!"reading above {level} failed: {reason}"
  | .ok (wholeHeight, entries), .ok (aboveHeight, found) =>
    let expected := entries.filter fun entry => level < entry.index
    if wholeHeight ≠ aboveHeight then
      return .error <|
        s!"--after {level} changed the reported height from {repr wholeHeight}" ++
          s!" to {repr aboveHeight}"
    else if found ≠ expected then
      return .error <|
        s!"--after {level} reported {heights found} where the whole stream minus" ++
          s!" that floor is {heights expected}"
    else
      return .ok ()

/--
Reading again reports everything reading once reported.

The first read is what makes an endpoint remember; this asserts the second is
not paid for out of what it hands back.
-/
def readingTwiceSubtractsNothing (door : Door) (stocked : Stocked) (level : Option UInt64) :
    IO (Except String Unit) := do
  let first ← readingOf door stocked level
  let again ← readingOf door stocked level
  match first, again with
  | .error reason, _ => return .error s!"the first read failed: {reason}"
  | _, .error reason => return .error s!"the second read failed: {reason}"
  | .ok before, .ok after =>
    if before = after then
      return .ok ()
    else
      return .error <|
        s!"reading twice with --after {repr level} gave" ++
          s!" ({repr before.1}, {heights before.2}) then ({repr after.1}, {heights after.2})"

end Kusanagi.Cairn
