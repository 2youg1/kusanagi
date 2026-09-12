/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Data.TreeMap
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground

/-!
# What a host measures when it cannot read anything

`Kusanagi.Lying` attacks the bytes. This module attacks the *shape* of them,
which is the attack a host gets for free: it never has to guess a key, break a
cipher or forge a signature. It weighs the parcels.

Five properties, each a relation between drops rather than an expected value:

* **One size.** Every drop is the same size, so the length of what somebody
  said is not a thing a host holds.
* **Never twice.** No two drops are byte-identical, which is what a repeated
  key or a repeated nonce looks like from outside.
* **No seam.** Two drops agree at about one byte in 256 and no more than the
  noise around that. A longer agreement means structure — a header, a version,
  a constant nonce, a keystream reused — and structure is what a detection
  rule is made of. See `tolerance` for what that bound reaches and what it
  does not.
* **No position is fixed.** Across many drops at once, no byte offset holds
  one value in all of them. This is the property `tolerance` cannot have: a
  four-byte field at a constant offset lifts a pairwise count by four, far
  under a noise floor of 113, and is invisible to every property above.
* **The pad is not a channel.** The same sentence sent twice leaves two drops
  with nothing in common. If the padding were ever left unchecked, or filled
  with anything but zeroes, the tails of those two drops would agree.

Every drop examined here is found through an address the sender's own `--json`
reported, never by listing the host's directory. A property that learned where
drops are by reading the host would quietly stop testing anything the day the
host's layout changed.
-/

namespace Kusanagi.Veil

open Std (TreeMap)
open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground

/--
How much two drops of `n` bytes may agree before the agreement means
something.

Two independent ChaCha20 keystreams agree at one byte in 256, so the count of
agreeing positions is binomial: it expects `n / 256` and has a standard
deviation of `sqrt (n * 255 / 256^2)`. The threshold is five deviations above
the expectation, which one pair clears by chance about three times in ten
million while a run compares barely a dozen pairs.

**It has to be a function of `n`, and it used to be the constant 64.** That
constant was calibrated when a drop was 4 096 bytes and chance explained
sixteen agreements. ML-DSA-87 pushed a drop to 131 072 bytes, chance began
explaining 512, and three properties here failed on every run against a build
with nothing wrong with it — the observed counts, 490 to 536, sit inside
512 ± 1.1 deviations. The noise floor grows with the square root of the drop,
so the slack above it must too, and the next change of signature scheme now
needs no edit here.

What this catches is agreement spread across the whole drop and larger than
the noise: a reused keystream, a constant nonce, a tail left in the clear.
What it does not catch is a short fixed field — a four-byte tag at the same
offset in every drop lifts the count by four, far under a noise floor of 113.
A leading header is caught by `prefixTolerance`, and a short one in the middle
by `noPositionIsFixed`, which compares which /positions/ agree across many
drops at once rather than how many agree in one pair.
-/
def tolerance (n : Nat) : Nat :=
  let expected := n / 256
  -- `sqrt` on a `Float` and back: the count is a natural number, and the
  -- deviation only has to be right to a byte.
  let deviation := (Float.ofNat n * 255.0 / 65536.0).sqrt.ceil.toUInt64.toNat
  expected + 5 * deviation

/--
The longest run of equal leading bytes two drops may share.

Zero would be too strict: two random strings share a first byte once in 256.
Four is out of reach by chance across the handful of drops a test writes, and
shorter than any header anybody would add.
-/
def prefixTolerance : Nat := 4

/-- The channel every property here opens. Names are local, so one will do. -/
private def channel : ChannelName := ⟨"peer"⟩

/-- How far two drops agree: everywhere, and from the front. -/
private structure Overlap where
  agreement : Nat
  leading : Nat
  intact : Bool

private def overlapOf (left right : ByteArray) : Overlap :=
  (List.range (min left.size right.size)).foldl
    (fun seen offset =>
      let same := left[offset]? == right[offset]?
      { agreement := if same then seen.agreement + 1 else seen.agreement
        leading := if same && seen.intact then seen.leading + 1 else seen.leading
        intact := same && seen.intact })
    { agreement := 0, leading := 0, intact := true }

/-- Why two drops are too alike, if they are. -/
def apart (left right : ByteArray) : Option String :=
  let width := min left.size right.size
  let seen := overlapOf left right
  if seen.agreement > tolerance width then
    some
      s!"two drops agree at {seen.agreement} byte positions, and chance explains up to \
         {tolerance width}; that is structure, and structure is a detection rule"
  else if seen.leading > prefixTolerance then
    some
      s!"two drops share a {seen.leading}-byte prefix; a header that long is all a \
         classifier needs"
  else
    none

def pairs (items : List α) : List (α × α) :=
  items.zipIdx.flatMap fun (item, index) => (items.drop (index + 1)).map (item, ·)

/-- Every reason any two of these drops are too alike. -/
private def alikeness (bodies : List ByteArray) : List String :=
  (pairs bodies).filterMap fun (left, right) => apart left right

/-- The sizes present, each named once, in order. -/
private def distinctSizes (bodies : List ByteArray) : List Nat :=
  ((bodies.map (·.size)).mergeSort (· ≤ ·)).foldr
    (fun size seen => match seen with
      | head :: _ => if head == size then seen else size :: seen
      | [] => [size])
    []

/-- Says one thing and reports where the sender says it put it. -/
private def say (door : Door) (writer : System.FilePath) (text : String) : IO (List Address) := do
  match ← Door.ask door writer (.send channel text) with
  | .accepted (.sent _ _ address) => return [address]
  | _ => return []

/-- The bytes at each of these addresses, in the order the addresses were given. -/
private def bodiesAt (held : List (Address × ByteArray)) (wanted : List Address) :
    List ByteArray :=
  let filed : TreeMap Address ByteArray :=
    held.foldl (fun below (address, body) => below.insert address body) ∅
  wanted.filterMap filed.get?

/-- Opens the channel both sides then use. -/
private def opened (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict := do
  match ← Door.ask door writer (.invite channel ground.waypoint .forever both) with
  | .accepted (.invited _ invitation _) =>
    match ← Door.ask door reader (.join invitation channel) with
    | .accepted (.joined ..) => return .held
    | other => return .broke s!"the channel could not be joined: {repr other}"
  | other => return .broke s!"the invitation was refused: {repr other}"

/--
Opens a channel, says one message of each length, and judges what was left.

The judge is handed exactly the drops the sender named, so a greeting or any
other traffic the protocol writes on its own is out of the sample and cannot
make a property pass by diluting it.
-/
private def written (door : Door) (ground : Ground) (writer reader : System.FilePath)
    (lengths : List Nat) (judge : List ByteArray → Verdict) : IO Verdict := do
  match ← opened door ground writer reader with
  | .held =>
    let addresses ← lengths.mapM fun length => say door writer ("".pushn 'x' length)
    let named := addresses.flatten
    let held ← ground.stored
    let bodies := bodiesAt held named
    if bodies.length == lengths.length then
      return judge bodies
    else
      return .broke
        s!"the sender reported {named.length} addresses and the host is holding \
           {bodies.length} of them"
  | refusal => return refusal

/--
Every drop is the same size, whatever it carries.

The messages differ by three orders of magnitude on purpose. A host that can
tell a one-byte remark from a three-thousand-byte one holds the shape of the
conversation, and a length profile survives encryption — it is how a censor
recognises a login, a photograph, a refusal.
-/
def sameSizeAlways (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict :=
  let lengths := [1, 7, 60, 500, 3000]
  written door ground writer reader lengths fun bodies =>
    match distinctSizes bodies with
    | [_] => .held
    | sizes =>
      .broke
        s!"messages of lengths {lengths} produced drops of sizes {sizes}; a host that \
           can measure an object can measure what was said"

/--
Everything the host is holding is one size, including what nobody reported.

The properties above judge the drops a sender named, which keeps a greeting or
any other protocol traffic out of the sample so that it cannot dilute them.
This one takes the opposite side deliberately: a host does not know which
objects were announced, so what it weighs is the whole store. An introduction
that is shorter than a message — or a build that grows one without growing the
other — marks the first object of every conversation, and the first object of
a conversation is the one that says a conversation began.
-/
def everyObjectIsOneSize (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict := do
  match ← opened door ground writer reader with
  | .held =>
    let _ ← say door writer ("".pushn 'x' 300)
    let _ ← say door reader "a short answer"
    let held ← ground.stored
    let bodies := held.map (·.2)
    match bodies.length, distinctSizes bodies with
    | 0, _ => return .broke "the host is holding nothing after a conversation"
    | _, [_] =>
      match alikeness bodies with
      | [] => return .held
      | reasons => return .broke (String.intercalate "\n" reasons)
    | _, sizes =>
      return .broke
        s!"the host is holding objects of sizes {sizes}; one of them is the \
           introduction, and a size that stands out marks where every conversation begins"
  | refusal => return refusal

/--
No two drops are the same bytes.

Invisible from the address side, and not subtle: a key or a nonce reused
across two drops makes identical plaintexts produce identical ciphertexts, so
a host that spots two equal objects has learnt that the same thing was said
twice without opening either.
-/
def neverTheSameBytesTwice (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict :=
  written door ground writer reader (List.replicate 6 32) fun bodies =>
    if (pairs bodies).any fun (left, right) => left.toList == right.toList then
      .broke "two drops are byte-identical; a key or a nonce was used twice"
    else
      .held

/--
No two drops share more than chance.

Any position where drops agree is a rule a censor can write, and finding it
costs one pass over a store. This is the property that fails on the day
somebody adds a magic number, a version byte or a length outside the envelope.
-/
def noSharedStructure (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict :=
  written door ground writer reader [10, 200, 1500, 40] fun bodies =>
    match alikeness bodies with
    | [] => .held
    | reasons => .broke (String.intercalate "\n" reasons)

/--
Every offset where `first` and all of `rest` carry the same byte.

Compared against the first drop rather than pairwise, which is the same
question asked once instead of `n choose 2` times: all of them agree at an
offset exactly when each of them agrees with the first one there.
-/
private def fixedOffsets (first : ByteArray) (rest : List ByteArray) : List Nat :=
  let width := rest.foldl (fun narrowest body => min narrowest body.size) first.size
  (List.range width).filter fun offset =>
    rest.all fun body => body[offset]? == first[offset]?

/--
No byte offset carries the same value in every drop.

**The property the pairwise bound cannot express.** `tolerance` asks how many
positions two drops agree at, and a fixed field of four bytes lifts that count
by four against a noise floor of 113 — invisible, at every drop size this
protocol will ever use. Comparing /which/ positions agree, across many drops
at once, turns the same field from a change of 4% of the noise into a
certainty: a constant is constant in all of them, and chance is not.

The threshold is therefore zero rather than a margin. Eight drops agree at one
offset by chance with probability `256 ^ -7`, so over a whole drop the expected
number of such offsets is about `2 * 10 ^ -12` — a test that fails once in
500 billion runs is a test that fails because something broke.

Any fixed offset is enough on its own: a version byte, a length outside the
envelope, a nonce that stopped varying, a tag somebody added for debugging. One
of them is the whole of what a censor needs, because a rule that matches a
single constant at a single offset costs one pass over a store.
-/
def noPositionIsFixed (door : Door) (ground : Ground) (writer reader : System.FilePath) :
    IO Verdict :=
  written door ground writer reader (List.replicate 8 64) fun bodies =>
    match bodies with
    | [] => .broke "no drops were written, so nothing was compared"
    | first :: rest =>
      match fixedOffsets first rest with
      | [] => .held
      | offsets =>
        .broke
          s!"{offsets.length} byte offset(s) hold one value in all {bodies.length} drops, \
             the first at {offsets.take 8}; a constant at a constant offset is a \
             detection rule"

/--
The same sentence, sent twice, leaves nothing in common behind.

Two heights, one key each. Resemblance would mean the derivation is not doing
its work; agreement confined to the tail would mean the padding is carrying
something — a counter, a build identifier, whatever a well-meaning patch put
there — and that padding is a covert channel nothing else would notice.
-/
def theSameSentenceTwiceSharesNothing (door : Door) (ground : Ground)
    (writer reader : System.FilePath) : IO Verdict := do
  match ← opened door ground writer reader with
  | .held =>
    let sentence := String.join (List.replicate 40 "the same thing ")
    let addresses ← [1, 2].mapM fun (_ : Nat) => say door writer sentence
    let held ← ground.stored
    match bodiesAt held addresses.flatten with
    | [first, second] =>
      return match apart first second with
        | none => .held
        | some reason => .broke reason
    | other =>
      return .broke
        s!"two drops were written and {other.length} were found at the addresses the \
           sender reported"
  | refusal => return refusal

/--
What a host measures without a key, run against one throwaway world each.

Each claim opens its own world, so a store one property filled cannot dilute
what the next one weighs.
-/
def suite (door : Door) : Suite :=
  let weighing (act : Door → Ground → System.FilePath → System.FilePath → IO Verdict) :
      IO Verdict :=
    withGround fun ground =>
      act door ground (ground.siteOf .alice) (ground.siteOf .bob)
  .group "what a host measures without a key"
    [ .claim "every drop is the same size" (weighing sameSizeAlways)
    , .claim "everything the host holds is one size, the introduction included"
        (weighing everyObjectIsOneSize)
    , .claim "no two drops are the same bytes" (weighing neverTheSameBytesTwice)
    , .claim "no two drops share structure" (weighing noSharedStructure)
    , .claim "no byte offset holds one value in every drop" (weighing noPositionIsFixed)
    , .claim "the same sentence twice shares nothing"
        (weighing theSameSentenceTwiceSharesNothing) ]

end Kusanagi.Veil
