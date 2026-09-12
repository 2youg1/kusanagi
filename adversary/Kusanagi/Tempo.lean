/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Discriminator
import Kusanagi.Door
import Kusanagi.Relay

/-!
# The same experiment as `Kusanagi.Discriminator`, from the other position

A host weighs objects. A carrier cannot: it holds nothing, opens nothing, and
under TLS does not even see an address. What it has is a list of moments, and
the question is whether one threshold on one number built from those moments
sorts the worlds into the two groups they were built as.

**Two positions, two lists of declared leaks, and they close on different
days.** The fixed-size envelope closed the host's size features years before
anything was done about rhythm; the public slot of `Roadmap.md` §I3 closes
rhythm without touching a single byte the host holds. Merging the two lists
would hide which mechanism was responsible for which line.
-/

namespace Kusanagi.Tempo

open Kusanagi.Check
open Kusanagi.Discriminator
open Kusanagi.Door
open Kusanagi.Relay

/--
The timing features that are allowed to separate silence from conversation.

**Measured, not predicted.** `gap.burst` came out at 3, 3, 3, 3 against 12, 12,
12, 12 — no overlap and no spread at all, because it is a count rather than a
duration. `gap.median` did not separate on any run: both worlds pay the same
process start for each command, so the middle of the distribution is the same in
a world that says nothing as in one that says three things. Writing down a leak
that does not happen is as wrong as leaving out one that does, because the
assertion below is an equality in both directions.

What it comes to: a command is a burst of requests, so the number of bursts is
the number of commands, which is the conversation. Nothing in the current design
can change that — an endpoint reaches the host exactly when its user reaches it.
**This is not the same mistake as the object count in
`Kusanagi.Discriminator`**, which was one fact reported in a second unit and
therefore not declarable: the carrier holds nothing and counts nothing else, so
this is its only channel, and the public slot of `Roadmap.md` §I3 closes it
outright by making the request times a function of the clock instead of of what
anybody said. The day that lands, this list becomes empty and the property below
is what says so.
-/
private def declared : List String := ["gap.burst"]

/--
A burst is a gap shorter than this, in nanoseconds.

A tenth of a second. `Kusanagi.Relay.Observation.observedAt` counts nanoseconds
rather than seconds, because that is the monotonic reading this toolchain
offers, so the threshold is written in the same unit rather than the duration
being converted at every comparison.
-/
private def burstWithin : Nat := 100000000

/-- The middle of these, or zero when there are none. -/
private def middle (values : List Nat) : Nat :=
  if values.isEmpty then 0
  else
    let ranked := values.mergeSort fun one other => decide (one ≤ other)
    (ranked.drop (values.length / 2)).head?.getD 0

/--
One number a carrier could put a threshold on.

The middle of the distribution and the clustering. A public slot flattens both
at once — every slot carries exactly one request whether or not anybody is
talking — so a mechanism that moved only one of them would show up here as one
line that stayed.

**The longest gap was measured, found to be an extremum, and removed.** It
separated on about one run in three, at 41–44 ms against 48–55 ms, and the
reason is arithmetic rather than anything about this product: the largest of
twelve draws exceeds the largest of three even when both come from the same
distribution, and the number of draws is the number of requests, which
`declared` already reports. Declaring it would have claimed a leak that no
design change could close, which is the mistake
`Kusanagi.Discriminator.constantPositions` records at the same rank in the other
feature set. **A statistic whose value depends on how many samples it saw is a
sample count in disguise.** Anything that grows with the sample — a maximum, a
range, a total — belongs here only after that dependence has been taken out of
it, and for a maximum over three samples there is no honest way to do that.

The median is reported in nanoseconds rather than in seconds. `separates` asks
whether the gap between two groups exceeds the spread inside either, and both
sides of that comparison carry the unit, so the choice of unit changes no
verdict.
-/
def timings (seen : List Observation) : List Reading :=
  let moments := seen.map (·.observedAt)
  -- Monotonic, so a later moment is never smaller than the one before it and
  -- the truncating subtraction on `Nat` never has anything to truncate.
  let gaps := (moments.tail.zip moments).map fun (after, before) => after - before
  [ { name := "gap.median", value := Float.ofNat (middle gaps) }
  , { name := "gap.burst"
    , value := Float.ofNat (gaps.filter (fun gap => decide (gap < burstWithin))).length } ]

/-- One world, as its carrier heard it. -/
private def kept (door : Door) (lengths : List Nat) : IO (Except String (List Reading)) := do
  return (← sampleWorld door lengths).map fun world => timings world.seen

/--
One slotted world, as its carrier heard it.

Two ticks, so that the second finds its slot already filled: a schedule that
fires twice in one period must still produce one drop, and this is where that is
measured rather than assumed.
-/
private def slotted (door : Door) (lengths : List Nat) : IO (Except String (List Reading)) := do
  return (← slottedWorld door 2 lengths).map fun world => timings world.seen

/-- One side of an experiment: `sides` worlds, or the first reason there is not. -/
private def group (build : List Nat → IO (Except String (List Reading))) (lengths : List Nat) :
    IO (Except String (List (List Reading))) := do
  let gathered ← (List.range sides).mapM fun _ => build lengths
  return gathered.mapM id

/-- These names in order, which is how two lists of them are compared. -/
private def sorted (names : List String) : List String :=
  names.mergeSort fun left right => compare left right != .gt

private def shown (names : List String) : String := String.intercalate ", " names

/-- A drop, or the listing of a bin of them: the two requests a read makes. -/
private def isDrop (path : String) : Bool := "/d/".isPrefixOf path || "/bin/".isPrefixOf path

/--
Whether anything was measured at all.

**This runs before the other two and exists because of how they fail.** A relay
that recorded nothing gives every world an empty feature vector, no feature
separates anything, and both properties below go green while testing precisely
nothing. So the first question is not about the product: it is whether this
instrument is plugged in.
-/
def everyRequestIsSeen (door : Door) : IO Verdict := do
  match ← sampleWorld door [40, 40] with
  | .error reason => return .broke reason
  | .ok world =>
    let seen := world.seen
    if seen.length < 2 then
      return .broke <|
        "the relay carried a whole conversation and recorded " ++
        s!"{seen.length}" ++ " requests. Every timing property below is green " ++
        "for this reason and not for a better one."
    else
      match (seen.map (·.observedPath)).filter (!isDrop ·) with
      | [] => return .held
      | strange =>
        return .broke <|
          "the endpoint asked the host for something that is not a drop: " ++
          s!"{strange.take 4}"

/--
Same number of messages, three orders of magnitude apart in what they say.

A fixed-size envelope makes them identical to the host. It should make them
identical to the carrier too, and a difference here would mean the size had come
back as a duration — a longer message taking longer to send is exactly the leak
the padding was bought to prevent.
-/
def volumeKeepsTime (door : Door) : IO Verdict := do
  match ← group (kept door) (List.replicate 4 1) with
  | .error reason => return .broke reason
  | .ok terse =>
    match ← group (kept door) (List.replicate 4 3000) with
    | .error reason => return .broke reason
    | .ok wordy =>
      match separating terse wordy with
      | [] => return .held
      | found =>
        return .broke <|
          "a carrier can tell four one-byte messages from four three-thousand-byte " ++
          "ones by their rhythm alone:\n" ++ report found terse wordy

/-- Nothing said, against something said, as a carrier hears it. -/
def presenceSaysOnlyWhatIsWrittenDown (door : Door) : IO Verdict := do
  match ← group (kept door) [] with
  | .error reason => return .broke reason
  | .ok quiet =>
    match ← group (kept door) (List.replicate 3 200) with
    | .error reason => return .broke reason
    | .ok talking =>
      let found := sorted (separating quiet talking)
      if found == sorted declared then
        return .held
      else
        return .broke <|
          "what a carrier hears between a silent channel and a busy one has changed.\n" ++
          "  written down: " ++ shown (sorted declared) ++ "\n" ++
          "  measured:     " ++ shown found ++ "\n" ++
          report found quiet talking ++
          "\nA timing feature that has started separating is a new leak on the path. " ++
          "One that has stopped is a slot somebody built, and this list is where " ++
          "that gets said out loud."

/--
The same question on a channel that writes to a clock instead of to a caller.

**This is what `declared` exists to be compared against.** On an on-demand
channel a silent world and a busy one differ in `gap.burst`, and that entry is
written down because it is real. A slotted channel is the mechanism that closes
it, so on one the list must be empty — every feature, both positions, no
exceptions.

The two worlds queue different amounts and tick the same number of times. That
asymmetry is the experiment: what a carrier hears must follow the ticks and not
the queue.
-/
def presenceSaysNothingOnASlottedChannel (door : Door) : IO Verdict := do
  match ← group (slotted door) [] with
  | .error reason => return .broke reason
  | .ok quiet =>
    match ← group (slotted door) (List.replicate 3 200) with
    | .error reason => return .broke reason
    | .ok talking =>
      match separating quiet talking with
      | [] => return .held
      | found =>
        return .broke <|
          "a slotted channel is supposed to make a silent world and a busy one the " ++
          "same thing to a carrier, and it did not:\n" ++
          report found quiet talking ++
          "\nThe declared list for a slotted channel is the empty set. A feature that " ++
          "separates here is a slot that is not doing its job, not a leak to be " ++
          "written down."

/--
The same, from the host's position rather than the carrier's.

A host counts objects. Under a slot the count follows the number of ticks and
nothing else, so two worlds that ticked the same number of times must hold the
same number of drops whatever either of them had to say.
-/
def volumeSaysNothingOnASlottedChannel (door : Door) : IO Verdict := do
  match ← slottedWorld door 2 [] with
  | .error reason => return .broke reason
  | .ok quiet =>
    match ← slottedWorld door 2 (List.replicate 3 200) with
    | .error reason => return .broke reason
    | .ok talking =>
      if quiet.held.length == talking.held.length then
        return .held
      else
        return .broke <|
          "a host counted " ++ s!"{quiet.held.length}" ++ " objects in a silent " ++
          "world and " ++ s!"{talking.held.length}" ++ " in a busy one, on a channel " ++
          "where both ticked twice. Under a slot the object count is a function of " ++
          "the schedule and of nothing else."

/-- What a carrier who never holds an object can still hear. -/
def suite (door : Door) : Suite :=
  .group "what a carrier hears"
    [ .claim "every request a conversation makes reaches the relay"
        (everyRequestIsSeen door)
    , .claim "how much was said does not change the rhythm" (volumeKeepsTime door)
    , .claim "whether anything was said changes the rhythm by exactly what is written down"
        (presenceSaysOnlyWhatIsWrittenDown door)
    , .claim "a slotted channel sounds the same silent as busy"
        (presenceSaysNothingOnASlottedChannel door)
    , .claim "a slotted channel weighs the same silent as busy"
        (volumeSaysNothingOnASlottedChannel door) ]

end Kusanagi.Tempo
