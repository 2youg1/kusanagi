/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi

/-!
# The suite, in the order that loses the least time when something breaks

The mutation check runs first, because every verdict below it is worth less
until it is green: a suite that cannot go red reports on nothing. The renderer
is checked next because it takes milliseconds and because a broken deliverable
makes every counterexample below it worthless. Random traces come after, then
the directed attack, then the host's simplest lie.

With no binary to drive, this exits successfully and says so. A gate that could
not run is not a gate that failed, and treating it as one is how a second
toolchain ends up blocking contributors who never touched it.
-/

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Dynamic
open Kusanagi.Ground
open Kusanagi.Model

/--
The counterexample this adversary found, as it was minimised.

Before the fix, the third and fourth steps passed: accepting your own invitation
gave one endpoint two local names for one stream — both derived from the same
secret and the same author — so a read handed back what that endpoint had itself
just written, as though a peer had said it. An agent reading its own output as
input is a feedback loop, not a conversation.

`join` now refuses at the first step of that, which is why the second action
renders as a refusal.
-/
def remembered : Actions Action :=
  sequenced World
    [ .invite .alice ⟨"one"⟩ .forever both
    , .join .alice ⟨0⟩ ⟨"two"⟩
    , .send .alice ⟨"one"⟩ "beta"
    , .read .alice ⟨"one"⟩ ]

/--
Rule four, made mechanical.

The renderer is the authority for that Rust file, and the file is the proof that
what the renderer emits compiles and passes. Neither can move without the other,
and neither is a place where a rule could be restated.
-/
def deliverable : IO Verdict := do
  if !coherent World remembered then
    return .broke "the committed trace is not one the model admits"
  let rendered := Kusanagi.Regression.render "an_endpoint_cannot_accept_its_own_invitation" remembered
  let path : System.FilePath := "../crates/kusanagi/tests/from_adversary.rs"
  if (← IO.getEnv "KUSANAGI_ACCEPT") == some "1" then
    IO.FS.writeFile path rendered
    return .held
  if !(← path.pathExists) then
    return .broke s!"there is no {path}; rerun with KUSANAGI_ACCEPT=1"
  let onDisk ← IO.FS.readFile path
  return ensure (rendered.replace "\r\n" "\n" == onDisk.replace "\r\n" "\n")
    "the committed Rust test is not what this adversary renders"

/-- Runs one command-line question against a throwaway world. -/
private def doorway (door : Door) (name : String)
    (act : Door → Ground → IO Verdict) : Suite :=
  .claim name (withGround fun ground => act door ground)

/-- Runs one weighing against a throwaway world, with both sites in hand. -/
private def weighing (door : Door) (name : String)
    (act : Door → Ground → System.FilePath → System.FilePath → IO Verdict) : Suite :=
  .claim name <| withGround fun ground =>
    act door ground (ground.siteOf .alice) (ground.siteOf .bob)

open Kusanagi

def properties (door : Door) (glass : Suite) : Suite :=
  .group "adversary"
    [ Bite.suite door
    , .claim "the committed Rust test is what this adversary renders" deliverable
    , Overheard.nothingIdentifyingReachesTheCommandLine
    , .group "the edges of the command line itself"
        [ doorway door "an invitation survives whatever a clipboard did to it"
            Doorway.invitationSurvivesAnyClipboard
        , doorway door "a pipe with nothing in it is answered rather than waited on"
            Doorway.anEmptyPipeIsAnswered
        , doorway door "a flood on stdin ends with a code rather than a buffer"
            Doorway.aFloodOnStdinIsAnswered
        , doorway door "an ability nobody defined is refused, and the refusal says what to pass"
            Doorway.anArgumentTheVerbCannotActOnIsRefused
        , doorway door "a mistyped flag leaves by the door every other failure leaves by"
            Doorway.aMistypedFlagLeavesByTheSameDoor
        , doorway door "a site nobody placed lands under this user's profile"
            Doorway.aSiteNobodyPlacedLandsUnderTheProfile
        , doorway door "a machine that will not say where data lives is asked rather than guessed"
            Doorway.aMachineThatWillNotSayIsAsked ]
    , .claim "a mistyped line is answerable, and its advice can be taken"
        (Hunt.keyboard door (mkStdGen 41))
    , .claim "what an agent pipes in comes back byte for byte"
        (Hunt.piping door (mkStdGen 43))
    , .claim "a reader that remembers is still told everything"
        (Hunt.remembering door (mkStdGen 47))
    , .claim "what one endpoint says is what the other hears"
        (Hunt.traces door (mkStdGen 53))
    , .claim "a revoked peer is never readable again"
        (Hunt.revocation door (mkStdGen 59))
    , .claim "a corrupted object is refused, not believed" (Hunt.tampering door)
    , .claim "genuine bytes at the wrong address are not a segment"
        (Hunt.transplanting door (mkStdGen 61))
    , .claim "a host cannot talk a reader down from a height"
        (Hunt.vanishing door (mkStdGen 67))
    , weighing door "one identity answers to one name on both sides"
        Naming.bothEndsAgreeOnWhoTheOtherIs
    , Veil.suite door
    , Discriminator.suite door
    , Tempo.suite door
    , Surface.surface door
    , glass ]

def main : IO UInt32 := do
  match ← Kusanagi.Door.discover with
  | none =>
    IO.println "skipped: no kusanagi binary to drive. Run `just adversary`, or set KUSANAGI_BIN."
    return 0
  | some door => run (properties door (← Surface.window door))
