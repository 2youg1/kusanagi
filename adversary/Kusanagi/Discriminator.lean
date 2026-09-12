/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Relay

/-!
# The experiment an adversary holding our source code would run

Every other property here asks whether the product obeys a rule. This one asks
the question that matters once the repository is public: **given as many
labelled examples as they care to generate, can somebody tell the two worlds
apart?** They can run the binary too. Whatever residual difference exists, they
will find it, and no amount of argument on our side changes that.

So the argument is replaced by a measurement. Two experiments, both against
real worlds built by the real binary:

* **Volume.** Two worlds with the same number of messages, one carrying a byte
  each and one carrying three thousand. Nothing may separate them. This is the
  claim the fixed-size envelope in `kusanagi_seal::veil` exists to make, and
  before that envelope every size feature below would have separated them at a
  glance.
* **Presence.** A channel where nothing is said, against one where something
  is. Here exactly one thing separates them — how many drops there are — and
  the property asserts it is the *only* thing. A leak that is measured and
  written down is a decision; a leak that is argued about is a hope.

**The assertion is an equality, so it bites in both directions.** A new feature
that starts separating turns it red, which is the regression case. A declared
leak that stops separating also turns it red, which is the day somebody lands
cover traffic and has to come here and say so.

**Why single-threshold rules rather than a classifier.** A stump that separates
the two groups *is* the rule a censor deploys: one number, one comparison, no
model to ship. If no stump separates them the practical attack is closed, and
the counterexample this module prints is a sentence a person can read instead
of a set of weights.
-/

namespace Kusanagi.Discriminator

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Relay

/--
How many worlds are built for each side of an experiment.

Four. A threshold that happens to order eight random numbers into two clean
groups turns up once in thirty-five tries, which is far too often across ten
features — so `separates` also demands a margin, and these two rules together
are what keep this from being a test that fails on Tuesdays.
-/
def sides : Nat := 4

/--
The features that are allowed to separate a silent channel from a busy one.

One fact in two units: the host holds one object per thing said, so counting
objects counts the conversation. Closing it needs traffic that does not depend
on whether anybody is talking, which is not built. Until it is, this list is
where that gap is recorded, and the property below fails if the list is wrong
in either direction.
-/
private def declared : List String := ["bytes.total", "drops"]

/-- One number a censor could put a threshold on. -/
structure Reading where
  name : String
  value : Float
  deriving Repr, Inhabited

private def count (xs : List α) : Float := Float.ofNat xs.length

/-- Each value present, named once, in order. -/
private def distinct [Ord α] (xs : List α) : List α :=
  (xs.mergeSort fun left right => compare left right != .gt).foldr
    (fun value seen => match seen with
      | head :: _ => if compare value head == .eq then seen else value :: seen
      | [] => [value])
    []

private def smallest (values : List Nat) : Nat := values.min?.getD 0

private def largest (values : List Nat) : Nat := values.max?.getD 0

private def mean (values : List UInt8) : Float :=
  if values.isEmpty then 0.0
  else (values.foldl (fun sum byte => sum + Float.ofNat byte.toNat) 0.0) / count values

/--
How often one byte equals the byte before it.

The cheapest compressibility test there is, and the one that matters: text,
structure and a pad of zeroes all repeat, and a keystream does not. Roughly one
in 256 for anything properly encrypted. This is the feature that caught
Shadowsocks, and it is here so that it never catches this.
-/
private def repeats (bodies : List ByteArray) : Float :=
  let adjacent := bodies.flatMap fun body =>
    let unpacked := body.toList
    unpacked.zip unpacked.tail
  if adjacent.isEmpty then 0.0
  else count (adjacent.filter fun (before, after) => before == after) / count adjacent

/--
Byte offsets holding one value in every object, beyond what chance explains.

A magic number, a version byte, a length outside the envelope or a constant
nonce all show up here and nowhere else.

**The raw count is the object count wearing a hat, and subtracting the chance
floor is what stops it being one.** `k` independent keystreams agree at a given
offset with probability `256^(1-k)`, so a world holding two drops expects 512
agreements across 131 072 bytes and a world holding five expects none at all. A
raw count therefore separates any two worlds whose object counts differ — which
is the fact `drops` already reports, in a third unit, and `declared` would have
had to call it a leak that no design change could ever close.

The question this was built to ask survives the correction: a field fixed by the
format sits at the same offset whatever the object count, so what a censor could
act on is agreement *above* the floor. Zero when there are fewer than two
objects, because one object agrees with itself everywhere and that is an
artefact of the sample rather than a fact about the product.
-/
private def constantPositions (bodies : List ByteArray) : Nat :=
  let held := bodies.length
  if held < 2 then 0 else
    let shortest := smallest (bodies.map (·.size))
    let agrees (offset : Nat) : Bool :=
      match bodies.map (fun body => body[offset]?) with
      | [] => false
      | first :: rest => rest.all (· == first)
    let agreeing := ((List.range shortest).filter agrees).length
    -- Binomial, as `Kusanagi.Veil.tolerance` is for the pairwise case: five
    -- deviations above the expectation, which chance clears about three times
    -- in ten million.
    let chance := (List.range (held - 1)).foldl (fun narrowing _ => narrowing / 256.0) 1.0
    let expected := Float.ofNat shortest * chance
    let chanceFloor :=
      (expected + 5.0 * (expected * (1.0 - chance)).sqrt).ceil.toUInt64.toNat
    -- Truncating subtraction on `Nat` is the `max 0` the floor needs: an
    -- agreement count under the floor is chance, and chance is not a feature.
    agreeing - chanceFloor

/--
Everything measurable in what a host holds, without holding any key.

Deliberately wider than what we expect to be closed. A feature nobody thought of
is the one that catches the next mistake, and carrying an extra measurement
costs one line.
-/
def features (held : List (Address × ByteArray)) : List Reading :=
  let bodies := held.map (·.2)
  let sizes := bodies.map (·.size)
  let everyByte := bodies.flatMap (·.toList)
  let nameLengths := held.map fun (address, _) => address.key.length
  [ { name := "drops", value := count bodies }
  , { name := "bytes.total", value := Float.ofNat (sizes.foldl (· + ·) 0) }
  , { name := "size.smallest", value := Float.ofNat (smallest sizes) }
  , { name := "size.largest", value := Float.ofNat (largest sizes) }
  , { name := "size.distinct", value := count (distinct sizes) }
  , { name := "name.length", value := Float.ofNat (largest nameLengths) }
  , { name := "byte.mean", value := mean everyByte }
  , { name := "byte.distinct", value := count (distinct everyByte) }
  , { name := "byte.repeats", value := repeats bodies }
  , { name := "constant.positions", value := Float.ofNat (constantPositions bodies) } ]

private def named (samples : List (List Reading)) : List String :=
  (samples.take 1).flatten.map (·.name)

private def valuesOf (what : String) (samples : List (List Reading)) : List Float :=
  samples.flatMap fun sample => (sample.filter (·.name == what)).map (·.value)

/-- The lowest and the highest of these, or nothing when there are none. -/
private def least (values : List Float) : Option Float :=
  values.foldl (fun seen value =>
    some (match seen with | none => value | some held => if value ≤ held then value else held)) none

private def most (values : List Float) : Option Float :=
  values.foldl (fun seen value =>
    some (match seen with | none => value | some held => if value ≤ held then held else value)) none

/--
Whether one threshold puts every value of one group past every value of the
other, by more than the spread inside either group.

The margin is what makes this a statement about a rule that would keep working.
Two groups can fall into clean order by luck; two groups separated by more than
their own variation cannot, and only the second kind is something an adversary
could deploy against traffic it has not seen.
-/
private def separates (left right : List Float) : Bool :=
  match least left, most left, least right, most right with
  | some lowLeft, some highLeft, some lowRight, some highRight =>
    let ahead := lowRight - highLeft
    let behind := lowLeft - highRight
    let gap := if ahead ≤ behind then behind else ahead
    let spreadLeft := highLeft - lowLeft
    let spreadRight := highRight - lowRight
    let spread := if spreadLeft ≤ spreadRight then spreadRight else spreadLeft
    decide (spread < gap)
  -- A group with nothing in it separates nothing.
  | _, _, _, _ => false

/-- The features on which one threshold classifies every world correctly. -/
def separating (left right : List (List Reading)) : List String :=
  (named left).filter fun what => separates (valuesOf what left) (valuesOf what right)

/-- These names in order, which is how two lists of them are compared. -/
private def sorted (names : List String) : List String :=
  names.mergeSort fun left right => compare left right != .gt

private def shown (names : List String) : String := String.intercalate ", " names

/-- What the two groups actually measured, for whoever reads the failure. -/
def report (names : List String) (left right : List (List Reading)) : String :=
  String.join <| names.map fun what =>
    let ranked (samples : List (List Reading)) : List Float :=
      (valuesOf what samples).mergeSort fun one other => decide (one ≤ other)
    s!"  {what}: {ranked left} against {ranked right}\n"

/--
One world, from both positions it can be watched from.

The host's view and the carrier's view of the same conversation. Building them
together is not an optimisation: a timing comparison against worlds built
separately from the ones that were weighed would be two experiments described as
one.
-/
structure Sample where
  held : List (Address × ByteArray)
  seen : List Observation

/-- The channel every world here opens. -/
private def channel : ChannelName := ⟨"peer"⟩

/--
A site inside a throwaway world.

Named here rather than taken from `Kusanagi.Ground`'s cast, because these
endpoints have no part to play: each world is built, measured and destroyed, and
a name would suggest a continuity that does not exist.
-/
private def siteIn (ground : Ground) (who : String) : System.FilePath :=
  System.FilePath.mk (ground.waypoint.toString ++ "-" ++ who)

/-- Says each of these lengths on an open channel, as one message apiece. -/
private def saying (door : Door) (writer : System.FilePath) (lengths : List Nat) : IO Unit :=
  lengths.forM fun length => do
    let _ ← Door.ask door writer (.send channel ("".pushn 'x' length))

/-- Opens a channel between two fresh endpoints and says these things on it. -/
private def converse (door : Door) (ground : Ground) (at? : String) (lengths : List Nat) :
    IO (Except String Unit) := do
  let writer := siteIn ground "one"
  let reader := siteIn ground "two"
  match ← Door.ask door writer (.invite channel (System.FilePath.mk at?) .forever both) with
  | .accepted (.invited _ invitation _) =>
    match ← Door.ask door reader (.join invitation channel) with
    | .accepted (.joined ..) =>
      saying door writer lengths
      return .ok ()
    | other => return .error s!"the channel could not be joined: {repr other}"
  | other => return .error s!"the invitation was refused: {repr other}"

/--
Opens a slotted channel, queues `lengths`, and ticks `ticks` times.

The period is an hour, so every tick after the first finds its slot already
filled and writes nothing. That is deliberate: what is being measured is the
traffic a *schedule* produces, and a schedule that fired twice in one period
must produce one drop, not two. A shorter period would measure the clock.
-/
private def converseSlotted (door : Door) (ground : Ground) (at? : String) (ticks : Nat)
    (lengths : List Nat) : IO (Except String Unit) := do
  let writer := siteIn ground "one"
  let reader := siteIn ground "two"
  match ← Door.ask door writer (.inviteEvery channel (System.FilePath.mk at?) 3600) with
  | .accepted (.invited _ invitation _) =>
    match ← Door.ask door reader (.join invitation channel) with
    | .accepted (.joined ..) =>
      saying door writer lengths
      (List.range ticks).forM fun _ => do
        let _ ← Door.ask door writer (.tick channel)
      return .ok ()
    | other => return .error s!"the slotted channel could not be joined: {repr other}"
  | other => return .error s!"the slotted invitation was refused: {repr other}"

/-- One throwaway world, measured and then deleted. -/
def sampleWorld (door : Door) (lengths : List Nat) : IO (Except String Sample) :=
  withGround fun ground =>
    withRelay door ground.waypoint fun relay => do
      match ← converse door ground relay.locator lengths with
      | .error reason => return .error reason
      | .ok () => return .ok { held := ← ground.stored, seen := ← relay.observed }

/--
One throwaway world on a slotted channel, driven by `tick` rather than by what
anybody has to say.

The parameter is what the writer *queues*, and the number of ticks is fixed
regardless: that is the whole experiment. A world with three messages and a
world with none must both produce exactly `ticks` drops, at the same rhythm,
because the slot is what decides both.
-/
def slottedWorld (door : Door) (ticks : Nat) (lengths : List Nat) : IO (Except String Sample) :=
  withGround fun ground =>
    withRelay door ground.waypoint fun relay => do
      match ← converseSlotted door ground relay.locator ticks lengths with
      | .error reason => return .error reason
      | .ok () => return .ok { held := ← ground.stored, seen := ← relay.observed }

/-- What a host holds in one world, or why there is no world. -/
private def weighed (door : Door) (lengths : List Nat) : IO (Except String (List Reading)) := do
  return (← sampleWorld door lengths).map fun sample => features sample.held

/-- One side of an experiment: `sides` worlds, or the first reason there is not. -/
private def group (door : Door) (lengths : List Nat) : IO (Except String (List (List Reading))) :=
  (List.range sides).foldlM
    (fun gathered _ => do
      match gathered with
      | .error reason => return .error reason
      | .ok so_far => return (← weighed door lengths).map fun readings => so_far ++ [readings])
    (.ok [])

/-- Same number of messages, three orders of magnitude apart in what they say. -/
def volumeSaysNothing (door : Door) : IO Verdict := do
  match ← group door (List.replicate 4 1) with
  | .error reason => return .broke reason
  | .ok terse =>
    match ← group door (List.replicate 4 3000) with
    | .error reason => return .broke reason
    | .ok wordy =>
      match separating terse wordy with
      | [] => return .held
      | found =>
        return .broke <|
          "a host can tell four one-byte messages from four three-thousand-byte " ++
          "ones, and needs no key to do it:\n" ++ report found terse wordy

/-- Nothing said, against something said. -/
def presenceSaysOnlyHowMany (door : Door) : IO Verdict := do
  match ← group door [] with
  | .error reason => return .broke reason
  | .ok quiet =>
    match ← group door (List.replicate 3 200) with
    | .error reason => return .broke reason
    | .ok talking =>
      let found := sorted (separating quiet talking)
      if found == sorted declared then
        return .held
      else
        return .broke <|
          "what separates a silent channel from a busy one has changed.\n" ++
          "  written down: " ++ shown (sorted declared) ++ "\n" ++
          "  measured:     " ++ shown found ++ "\n" ++
          report found quiet talking ++
          "\nA feature that has started separating is a new leak. One that has " ++
          "stopped is a leak somebody closed, and this list is where that gets " ++
          "said out loud."

/-- What a classifier trained on this repository would find. -/
def suite (door : Door) : Suite :=
  .group "what a classifier trained on this repository would find"
    [ .claim "how much was said does not separate two worlds" (volumeSaysNothing door)
    , .claim "whether anything was said separates them by exactly what is written down"
        (presenceSaysOnlyHowMany door) ]

end Kusanagi.Discriminator
