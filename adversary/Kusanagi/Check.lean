/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/

/-!
# Generating cases, shrinking them, and saying which claim broke

Generating cases, narrowing a failing one, and naming which claim broke are
three jobs this suite needs and the toolchain does not provide, so they live
here. It stays small because `Init.Data.Random` already supplies the splittable
generator the whole idea rests on.

Two things are deliberately absent. There is no decision procedure and no
proof: every claim in this suite is settled by running the shipped binary and
comparing two traces, so a `Prop` would only be a shape to put around an `IO`
answer. And there is no reporter protocol: the tree prints itself.
-/

namespace Kusanagi.Check

/-- A random value, drawn at a size the caller controls. -/
structure Gen (α : Type) where
  draw : StdGen → Nat → α × StdGen

namespace Gen

instance : Functor Gen where
  map f gen := ⟨fun seed size => let (value, seed) := gen.draw seed size; (f value, seed)⟩

instance : Monad Gen where
  pure value := ⟨fun seed _ => (value, seed)⟩
  bind gen f := ⟨fun seed size =>
    let (value, seed) := gen.draw seed size
    (f value).draw seed size⟩

/-- The size this draw was asked for, which is how a trace decides its length. -/
def sized (f : Nat → Gen α) : Gen α := ⟨fun seed size => (f size).draw seed size⟩

/-- Draws at a fixed size, whatever the caller was working at. -/
def resize (size : Nat) (gen : Gen α) : Gen α := ⟨fun seed _ => gen.draw seed size⟩

/-- A number in `[lo, hi]`, inclusive at both ends. -/
def choose (lo hi : Nat) : Gen Nat := ⟨fun seed _ => randNat seed lo hi⟩

/--
One of these, uniformly.

The first element sits apart from the rest so that the list cannot be empty.
An `elements` over nothing has no answer, and a default substituted here would
be a value no generator chose.
-/
def elements (first : α) (rest : List α) : Gen α := do
  let index ← choose 0 rest.length
  return ((first :: rest)[index]?).getD first

/-- One of these, in proportion to the weights. Zero-weighted branches never run. -/
def frequency (first : Nat × Gen α) (rest : List (Nat × Gen α)) : Gen α := do
  let weighted := first :: rest
  let total := weighted.foldl (fun sum (weight, _) => sum + weight) 0
  if total == 0 then
    first.2
  else
    let mark ← choose 1 total
    let rec pick (remaining : List (Nat × Gen α)) (mark : Nat) : Gen α :=
      match remaining with
      | [] => first.2
      | (weight, gen) :: rest => if mark ≤ weight then gen else pick rest (mark - weight)
    pick weighted mark

/-- One of these generators, uniformly. -/
def oneOf (first : Gen α) (rest : List (Gen α)) : Gen α := do
  let index ← choose 0 rest.length
  ((first :: rest)[index]?).getD first

/-- Exactly this many. -/
def listOfLength (count : Nat) (gen : Gen α) : Gen (List α) :=
  match count with
  | 0 => pure []
  | n + 1 => do return (← gen) :: (← listOfLength n gen)

/-- Between none and the current size. -/
def listOf (gen : Gen α) : Gen (List α) := sized fun size => do
  listOfLength (← choose 0 size) gen

/-- Heads this often, in hundredths. -/
def chance (percent : Nat) : Gen Bool := do return (← choose 1 100) ≤ percent

def byte : Gen UInt8 := do return UInt8.ofNat (← choose 0 255)

def bytes : Gen ByteArray := do return ByteArray.mk (← listOf byte).toArray

end Gen

/-- Simpler values to try once a case has broken a claim. -/
class Shrinkable (α : Type) where
  shrink : α → List α := fun _ => []

export Shrinkable (shrink)

instance : Shrinkable Nat where
  shrink n := if n == 0 then [] else (List.range n).filter fun candidate =>
    candidate == 0 || candidate == n / 2 || candidate + 1 == n

instance : Shrinkable Bool where
  shrink b := if b then [false] else []

instance : Shrinkable UInt8 where
  shrink b := (shrink b.toNat).map UInt8.ofNat

instance : Shrinkable String where
  shrink text :=
    let letters := text.toList
    (List.range letters.length).map fun index =>
      String.ofList (letters.eraseIdx index)

instance [Shrinkable α] [Shrinkable β] : Shrinkable (α × β) where
  shrink pair :=
    (shrink pair.1).map (·, pair.2) ++ (shrink pair.2).map (pair.1, ·)

instance [Shrinkable α] : Shrinkable (Option α) where
  shrink
    | none => []
    | some value => none :: (shrink value).map some

instance [Shrinkable α] : Shrinkable (List α) where
  shrink xs :=
    -- Drop one, then simplify one. Dropping first is what makes a shrunk trace
    -- short enough to read.
    (List.range xs.length).map (fun index => xs.eraseIdx index) ++
      (List.range xs.length).flatMap fun index =>
        match xs[index]? with
        | none => []
        | some found => (shrink found).map fun smaller => xs.set index smaller

instance : Shrinkable ByteArray where
  shrink array := (shrink array.toList).map fun smaller => ByteArray.mk smaller.toArray

/-- What one run of a claim established. -/
inductive Verdict where
  | held
  /-- The claim is false, and this is the case that shows it. -/
  | broke (why : String)
  /-- Nothing was asked, because what it needs is not here. -/
  | skipped (why : String)
  deriving Inhabited

def Verdict.isBroken : Verdict → Bool
  | .broke _ => true
  | _ => false

/-- Holds when the two sides agree, and names both when they do not. -/
def equals [BEq α] [ToString α] (what : String) (seen expected : α) : Verdict :=
  if seen == expected then .held
  else .broke s!"{what}: saw {seen} where {expected} was owed"

/-- Holds when the condition does. -/
def ensure (holds : Bool) (why : String) : Verdict :=
  if holds then .held else .broke why

/-- The size a draw is made at, which grows as a run goes on. -/
private def sizeFor (index runs : Nat) : Nat := 1 + (index * 30) / (max runs 1)

/--
Draws cases until one breaks the claim, then shrinks it as far as it goes.

Shrinking is greedy: it stops at the first candidate that no longer breaks
anything, and starts again from the one that does. It gives up after
`shrinkLimit` accepted steps so that a generator with a rich shrink tree cannot
turn one failure into an unbounded run.
-/
private partial def narrow [Shrinkable α] (check : α → IO Verdict) :
    List α → IO (Option (α × String))
  | [] => return none
  | candidate :: rest => do
    match ← check candidate with
    | .broke why => return some (candidate, why)
    | _ => narrow check rest

private partial def minimise [Shrinkable α] (check : α → IO Verdict) (render : α → String)
    (shrinkLimit : Nat) (case : α) (why : String) (steps : Nat) : IO Verdict := do
  if steps ≥ shrinkLimit then
    return .broke s!"{render case}\n  {why}"
  match ← narrow check (shrink case) with
  | some (smaller, smallerWhy) =>
    minimise check render shrinkLimit smaller smallerWhy (steps + 1)
  | none => return .broke s!"{render case}\n  {why}"

partial def forAll [Shrinkable α] (runs : Nat) (gen : Gen α) (render : α → String)
    (check : α → IO Verdict) (seed : StdGen) (shrinkLimit : Nat := 200) : IO Verdict := do
  let rec attempt (index : Nat) (seed : StdGen) : IO Verdict := do
    if index ≥ runs then
      return .held
    let (case, next) := gen.draw seed (sizeFor index runs)
    match ← check case with
    | .held => attempt (index + 1) next
    | .skipped why => return .skipped why
    | .broke why => minimise check render shrinkLimit case why 0
  attempt 0 seed

/-- A named claim, or a named group of them. -/
inductive Suite where
  | claim (name : String) (settle : IO Verdict)
  | group (name : String) (children : List Suite)

/-- One claim, addressed by the groups it sits under. -/
private structure Leaf where
  path : List String
  name : String
  settle : IO Verdict

private def Suite.leaves (suite : Suite) (path : List String := []) : List Leaf :=
  match suite with
  | .claim name settle => [{ path, name, settle }]
  | .group name children => children.flatMap (·.leaves (path ++ [name]))

/--
How many claims run at once.

Every claim here spawns the real binary, so sixteen at a time measures the
scheduler rather than the product, and the timing experiment is the one that
would notice. Isolation is by construction — one temporary directory per world,
one operating-system-assigned port per relay — so the only reason this number
is not one is that a suite which never overlapped was leaving most of a machine
idle.
-/
def abreast : Nat := 4

private def Leaf.run (leaf : Leaf) : IO (Leaf × Verdict) := do
  let verdict ←
    try leaf.settle
    catch failure => pure (.broke s!"the claim itself raised: {failure}")
  return (leaf, verdict)

/-- Runs every claim, `abreast` at a time, and returns them in the order written. -/
private partial def harvest : List Leaf → IO (List (Leaf × Verdict))
  | [] => return []
  | remaining => do
    let batch := remaining.take abreast
    let started ← batch.mapM fun leaf => IO.asTask leaf.run Task.Priority.dedicated
    let mut done := []
    for task in started do
      done := done ++ [← IO.ofExcept task.get]
    return done ++ (← harvest (remaining.drop abreast))

/--
Runs a suite, prints what each claim did, and answers with the exit code.

A skipped claim is not a failed one. The whole directory skips itself when there
is no binary to drive, and a gate that could not run must not look like a gate
that failed — that is how a second toolchain ends up blocking contributors who
never touched it.
-/
def run (suite : Suite) : IO UInt32 := do
  let leaves := suite.leaves
  let results ← harvest leaves
  let mut shown : List String := []
  let mut broken := 0
  let mut skipped := 0
  for (leaf, verdict) in results do
    if leaf.path != shown then
      shown := leaf.path
      for (step, depth) in leaf.path.zipIdx do
        IO.println s!"{"".pushn ' ' (2 * depth)}{step}"
    let indent := "".pushn ' ' (2 * leaf.path.length)
    match verdict with
    | .held => IO.println s!"{indent}  ok    {leaf.name}"
    | .skipped why =>
      skipped := skipped + 1
      IO.println s!"{indent}  skip  {leaf.name} — {why}"
    | .broke why =>
      broken := broken + 1
      IO.println s!"{indent}  BROKE {leaf.name}"
      for line in why.splitOn "\n" do
        IO.println s!"{indent}        {line}"
  let held := leaves.length - broken - skipped
  IO.println ""
  IO.println s!"{held} held, {broken} broke, {skipped} skipped, of {leaves.length}"
  return if broken == 0 then 0 else 1

end Kusanagi.Check
