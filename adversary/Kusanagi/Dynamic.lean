/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Check

/-!
# A model, the traces it admits, and a way to aim one

A model of the product, the traces that model admits, and a way to aim one at a
particular attack. Two ideas carry the whole module.

The first is that **generating a trace and running it are different jobs with
different types**. `StateModel` knows what may be attempted and what the world
becomes; `RunModel` knows how to attempt it against the real program. Nothing
in `StateModel` can reach the binary, which is what makes the black box a fact
about the types rather than about anybody's discipline.

The second is **polarity**. A step the model expects to be refused is worth
running, because the refusal is the promise: an error code is part of the
door's contract, and a suite that only ever ran what should succeed would never
check one. A negative step does not advance the model — what was refused did
not happen.

Aiming a trace is `Script` below: a generator that can pin some steps and
quantify over the rest, so that "any prefix, this attack, any suffix" is a
value rather than a comment. Uniform random traces reach an interesting state
rarely and by accident; naming the attack is the difference between a fuzzer
and an adversary.
-/

namespace Kusanagi.Dynamic

open Kusanagi.Answer
open Kusanagi.Check

/-- A value some earlier step produced, named by the step that produced it. -/
structure Var where
  step : Nat
  deriving DecidableEq, Ord, Repr, Inhabited

instance : ToString Var := ⟨fun v => s!"var{v.step}"⟩

/-- Whether a step is expected to be done, or expected to be refused. -/
inductive Polarity where
  | positive
  | negative
  deriving DecidableEq, Repr, Inhabited

/-- One attempt in a trace, and what the model says will become of it. -/
structure Step (Action : Type) where
  action : Action
  polarity : Polarity
  deriving Inhabited

/-- A trace: what to attempt, in order. -/
abbrev Actions (Action : Type) := List (Step Action)

/--
What may be attempted, and what the world becomes when it is.

Nothing here can reach the binary. A `StateModel` that could would be a second
authority for a rule that already has one in Rust.
-/
class StateModel (World : Type) (Action : outParam Type) where
  initial : World
  /-- Draws one candidate. `none` when the world admits nothing worth trying. -/
  arbitraryAction : World → Gen (Option Action)
  shrinkAction : World → Action → List Action := fun _ _ => []
  /-- Whether this may be attempted at all, quite apart from its outcome. -/
  attemptable : World → Action → Bool := fun _ _ => true
  /-- Whether the model expects this to succeed here. -/
  precondition : World → Action → Bool
  /-- Whether the model expects this to be refused here, which is also worth running. -/
  validFailingAction : World → Action → Bool
  next : World → Action → Var → World
  described : Action → String

export StateModel (initial arbitraryAction attemptable precondition validFailingAction next
  described)

/-- The polarity the model gives an action in a world, if it admits it at all. -/
def polarityOf (World : Type) {Action : Type} [StateModel World Action]
    (world : World) (action : Action) : Option Polarity :=
  if !attemptable world action then none
  else if precondition world action then some .positive
  else if validFailingAction world action then some .negative
  else none

/-- Walks a trace through the model, refusing the first step the model does not admit. -/
def replay (World : Type) {Action : Type} [StateModel World Action]
    (actions : Actions Action) : Option World :=
  let rec go (world : World) (index : Nat) : Actions Action → Option World
    | [] => some world
    | step :: rest =>
      match polarityOf World world step.action with
      | some polarity =>
        if polarity != step.polarity then none
        else
          let world := match polarity with
            | .positive => next world step.action ⟨index⟩
            | .negative => world
          go world (index + 1) rest
      | none => none
  go (initial : World) 0 actions

/-- Whether every step is one the model admits, with the polarity it was given. -/
def coherent (World : Type) {Action : Type} [StateModel World Action]
    (actions : Actions Action) : Bool :=
  (replay World actions).isSome

/--
Gives each action the polarity the model says it has, and drops what it will
not admit.

Polarity is decided here rather than written by hand, because a trace whose
polarity a person chose would be asserting the author's belief instead of the
model's.
-/
def sequenced (World : Type) {Action : Type} [StateModel World Action]
    (candidates : List Action) : Actions Action :=
  let rec go (world : World) (index : Nat) : List Action → Actions Action
    | [] => []
    | action :: rest =>
      match polarityOf World world action with
      | none => go world index rest
      | some .positive =>
        ⟨action, .positive⟩ :: go (next world action ⟨index⟩) (index + 1) rest
      | some .negative => ⟨action, .negative⟩ :: go world (index + 1) rest
  go (initial : World) 0 candidates

/-- The world a trace is built in while it is being generated. -/
structure Building (World Action : Type) where
  world : World
  taken : Actions Action
  length : Nat

/--
A generator that may pin some steps and quantify over the rest.

The state monad carries the model forward as the script is read, so a pinned
step and a quantified one advance the world the same way.
-/
abbrev Script (World Action : Type) := StateT (Building World Action) Gen

namespace Script

variable {World Action : Type} [StateModel World Action]

/-- The model as it stands at this point in the script. -/
def modelState : Script World Action World := return (← get).world

private def append (action : Action) (polarity : Polarity) :
    Script World Action Var := do
  let built ← get
  let variable' : Var := ⟨built.length⟩
  let world := match polarity with
    | .positive => next built.world action variable'
    | .negative => built.world
  set ({ world, taken := built.taken ++ [⟨action, polarity⟩], length := built.length + 1 }
    : Building World Action)
  return variable'

/--
Pins one step that must succeed.

A step the model would refuse here is dropped rather than forced: a script that
insisted on an impossible action would generate traces the model cannot explain
and fail for a reason that is about the script.
-/
def step (action : Action) : Script World Action Var := do
  let built ← get
  if precondition built.world action && attemptable built.world action then
    append action .positive
  else
    return ⟨built.length⟩

/-- Pins one step that must be refused. -/
def failing (action : Action) : Script World Action Unit := do
  let built ← get
  if validFailingAction built.world action && attemptable built.world action then
    let _ ← append action .negative

/-- Any number of arbitrary steps, up to the size the draw was asked for. -/
def anyActions : Script World Action Unit := do
  let howMany ← liftM (Gen.sized fun size => Gen.choose 0 size)
  for _ in List.range howMany do
    let built ← get
    match ← liftM (arbitraryAction (Action := Action) built.world) with
    | none => pure ()
    | some candidate =>
      match polarityOf World built.world candidate with
      | none => pure ()
      | some polarity => let _ ← append candidate polarity
  return ()

end Script

/--
Runs a script against a chosen starting world and keeps the trace it built.

The starting world is a parameter because a model that has been made to forget
one of its own rules is still a model, and asking what it generates is how the
rule is shown to be load-bearing.
-/
def forAllScriptFrom {World Action : Type} [StateModel World Action] (start : World)
    (script : Script World Action α) : Gen (Actions Action) := do
  let (_, built) ← StateT.run script
    ({ world := start, taken := [], length := 0 } : Building World Action)
  return built.taken

/-- Runs a script from the model's own initial world. -/
def forAllScript (World : Type) {Action : Type} [StateModel World Action]
    (script : Script World Action α) : Gen (Actions Action) :=
  forAllScriptFrom (initial : World) script

/-- A trace of arbitrary steps, which is `anyActions` and nothing else. -/
def arbitraryActions (World : Type) {Action : Type} [StateModel World Action] :
    Gen (Actions Action) :=
  forAllScript World (Script.anyActions (World := World) (Action := Action))

/--
Shrinks a trace by dropping steps, keeping only what the model still explains.

Dropping before simplifying is what makes a counterexample short enough to
read, and re-checking coherence is what stops a shrunk trace from presenting an
invitation that the dropped step would have minted.
-/
def shrinkActions (World : Type) {Action : Type} [StateModel World Action] [BEq Action]
    (actions : Actions Action) : List (Actions Action) :=
  let dropped := (List.range actions.length).map fun index =>
    sequenced World ((actions.eraseIdx index).map (·.action))
  let simplified := (List.range actions.length).flatMap fun index =>
    match actions[index]? with
    | none => []
    | some step =>
      (StateModel.shrinkAction (World := World) (initial : World) step.action).map
        fun smaller =>
          sequenced World ((actions.set index ⟨smaller, step.polarity⟩).map (·.action))
  (dropped ++ simplified).filter fun candidate =>
    candidate.length < actions.length ||
      candidate.map (·.action) != actions.map (·.action)

/--
How to attempt an action for real, and what must hold once it has been.

`Setting` is what it takes to run a trace at all — a built binary and a world to
run it in. It is a parameter rather than part of the instance, because a class
instance is global and the binary is not.
-/
class RunModel (Setting World Action Realized : Type) where
  /-- Attempts one action. The lookup reaches values earlier steps produced. -/
  perform : Setting → World → Action → (Var → Option Realized) →
    IO (Except Complaint Realized)
  /-- What must hold after a step the model expected to succeed. -/
  postcondition : (before after : World) → Action → Realized → IO Verdict
  /-- What must hold after a step the model expected to be refused. -/
  postconditionOnFailure : (before : World) → Action → Except Complaint Realized → IO Verdict

/-- Renders a trace the way a counterexample should read. -/
def render (World : Type) {Action : Type} [StateModel World Action]
    (actions : Actions Action) : String :=
  String.intercalate "\n" <| actions.zipIdx.map fun (step, index) =>
    let mark := match step.polarity with
      | .positive => s!"var{index} <- "
      | .negative => "refused: "
    s!"  {mark}{described (World := World) step.action}"

/--
Runs a trace against the real program and stops at the first step that broke.

The environment only ever holds what a step produced, so a step that reaches
for a variable an earlier step did not fill gets `none` and the model's own
`attemptable` is what stopped that from being generated.
-/
def runActionsFrom {World Setting Action Realized : Type} [StateModel World Action]
    [RunModel Setting World Action Realized] (setting : Setting) (start : World)
    (actions : Actions Action) : IO Verdict := do
  let mut world : World := start
  let mut produced : Array (Option Realized) := #[]
  for (step, index) in actions.zipIdx do
    let look : Var → Option Realized := fun v => (produced[v.step]?).join
    let attempted ← RunModel.perform setting world step.action look
    let after : World := match step.polarity with
      | .positive => next (World := World) world step.action ⟨index⟩
      | .negative => world
    let verdict : Verdict ←
      match step.polarity, attempted with
      | .positive, .ok realized =>
        RunModel.postcondition (Setting := Setting) world after step.action realized
      | .positive, .error complaint =>
        pure (Verdict.broke s!"this was owed, and was refused with {complaint.code}")
      | .negative, outcome =>
        RunModel.postconditionOnFailure (Setting := Setting) world step.action outcome
    match verdict with
    | .held => pure ()
    | .skipped why => return .skipped why
    | .broke why =>
      return .broke s!"{render World (actions.take (index + 1))}\n  step {index}: {why}"
    produced := produced.push attempted.toOption
    world := after
  return .held

/-- Runs a trace from the model's own initial world. -/
def runActions (World : Type) {Setting Action Realized : Type} [StateModel World Action]
    [RunModel Setting World Action Realized] (setting : Setting)
    (actions : Actions Action) : IO Verdict :=
  runActionsFrom (Realized := Realized) setting (initial : World) actions

/--
Draws traces until one breaks, then shrinks it as far as the model still
explains it.

Running a trace is the caller's job rather than this function's, because every
trace needs a world of its own — one temporary directory, one host — and a
setting built once and shared would make two traces interfere in ways that read
as broken rules. The caller builds the world, runs the trace in it, and may add
what it wants to assert about what the host was left holding.
-/
partial def huntWith (World : Type) {Action : Type} [StateModel World Action] [BEq Action]
    (generate : Gen (Actions Action)) (runs : Nat) (seed : StdGen)
    (attempt : Actions Action → IO Verdict) (shrinkLimit : Nat := 60) : IO Verdict := do
  let rec narrow : List (Actions Action) → IO (Option (Actions Action × String))
    | [] => return none
    | candidate :: rest => do
      match ← attempt candidate with
      | .broke why => return some (candidate, why)
      | _ => narrow rest
  let rec minimise (trace : Actions Action) (why : String) (steps : Nat) : IO Verdict := do
    if steps ≥ shrinkLimit then
      return .broke why
    match ← narrow (shrinkActions World trace) with
    | some (smaller, smallerWhy) => minimise smaller smallerWhy (steps + 1)
    | none => return .broke why
  let rec draw (index : Nat) (seed : StdGen) : IO Verdict := do
    if index ≥ runs then
      return .held
    let (trace, next) := generate.draw seed (4 + index % 9)
    match ← attempt trace with
    | .held => draw (index + 1) next
    | .skipped why => return .skipped why
    | .broke why => minimise trace why 0
  draw 0 seed

end Kusanagi.Dynamic
