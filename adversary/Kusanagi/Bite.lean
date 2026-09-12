/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Model

/-!
# Evidence that the model can still be wrong

A property that is always true is worth exactly as much as no property, and a
suite full of them reports green for a product nobody is checking. `adversary-SPEC.md`
§2.5 has always demanded this evidence; until now it demanded it of a reader.

Each claim here takes one rule out of `Kusanagi.Model.refusal`, runs a trace
that the rule is about, and requires two things of it: **the sharp model agrees
with the product, and the blunt model contradicts it.** Both halves are load
bearing. Without the first, a red result could come from the trace being
impossible rather than from the rule mattering. Without the second, nothing has
been shown at all.

The traces are directed rather than random. A random trace reaches a
self-accepted invitation eventually and by accident; naming the situation makes
the check deterministic, fast, and honest about what it proves.
-/

namespace Kusanagi.Bite

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Dynamic
open Kusanagi.Ground
open Kusanagi.Model

private def one : ChannelName := ⟨"one"⟩
private def two : ChannelName := ⟨"two"⟩

/--
A read after the peer was cut off.

Sharp, the last step is a refusal the model predicts, so it is dropped from a
positive script. Blunt, the model expects it to work and the product refuses.
-/
def readAfterRevoke : Script World Action Unit := do
  let ticket ← Script.step (.invite .alice one .forever both)
  let _ ← Script.step (.join .bob ticket two)
  let _ ← Script.step (.read .alice one)
  let _ ← Script.step (.revoke .alice one)
  let _ ← Script.step (.read .alice one)

/-- An endpoint accepting the invitation it minted itself, under a second name. -/
def selfAcceptance : Script World Action Unit := do
  let ticket ← Script.step (.invite .alice one .forever both)
  let _ ← Script.step (.join .alice ticket two)

/-- An endpoint that was granted reading only, speaking anyway. -/
def speakingWithoutTheGrant : Script World Action Unit := do
  let ticket ← Script.step (.invite .alice one .forever readOnly)
  let _ ← Script.step (.join .bob ticket two)
  let _ ← Script.step (.send .bob two "alpha")

/-- How long a trace this check draws. The scripts here pin every step. -/
private def drawnAt : Nat := 8

/--
Requires the sharp model to agree with the product and the blunt one to fall out
with it.

Each call takes a ground of its own. Two claims sharing one would be two
endpoints writing one site directory at once, and what comes back from that is a
local input-output failure wearing the clothes of a broken rule.

A `skipped` from either run is passed along: the binary is missing, and a check
that could not run must not look like a check that failed.
-/
def bites (door : Door) (blunting : Blunting) (script : Script World Action Unit)
    (seed : StdGen) : IO Verdict := withGround fun ground => do
  let kit : Kit := { door, ground }
  let sharp : World := {}
  let blunt : World := { blunted := blunting }
  let (asWritten, seed) := (forAllScriptFrom sharp script).draw seed drawnAt
  match ← runActionsFrom (Realized := Realized) kit sharp asWritten with
  | .skipped why => return .skipped why
  | .broke why =>
    return .broke s!"the model as written already disagrees with the product, so this \
      check proves nothing about {repr blunting}:\n  {why}"
  | .held =>
    let (asBlunted, _) := (forAllScriptFrom blunt script).draw seed drawnAt
    match ← runActionsFrom (Realized := Realized) kit blunt asBlunted with
    | .skipped why => return .skipped why
    | .broke _ => return .held
    | .held =>
      return .broke s!"the model forgot {repr blunting} and the suite still agreed with \
        the product, so that rule is carrying no weight"

/--
The suite that answers "can this still go red".

It runs first, because every verdict below it is worth less until this one is
green.
-/
def suite (door : Door) : Suite :=
  .group "the model can still be wrong"
    [ .claim "forgetting that revocation is final is noticed"
        (bites door .revocationCuts readAfterRevoke (mkStdGen 20260912))
    , .claim "forgetting that nobody accepts their own invitation is noticed"
        (bites door .ownInvitation selfAcceptance (mkStdGen 20260913))
    , .claim "forgetting that speaking needs the grant is noticed"
        (bites door .sendNeedsGrant speakingWithoutTheGrant (mkStdGen 20260914))
    ]

end Kusanagi.Bite
