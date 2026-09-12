/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Check
import Kusanagi.Door

/-!
# What the account at the next desk reads while a command runs

The adversary of this module is not the host and not the network. It is
another account on the same machine, which on Linux reads any process's
arguments out of `/proc` and afterwards reads the shell history the first
account left behind. It costs nothing and needs no privileges.

`ARCHITECTURE.md` §8 ruled the invitation off the command line for exactly
that reason. A channel name is worse: an invitation leaks one chance to enter
one channel, while `send --to bob` leaks who is talking to whom on every
message — the relationship graph that derived addresses exist to hide.

So this asserts the shape of the command line itself. It is a pure property
and takes microseconds, and it holds the door every other property in this
suite now walks through: `Kusanagi.Door` drives every verb with the name on
stdin, so a regression here fails seventeen tests rather than one.
-/

namespace Kusanagi.Overheard

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door

/-- Every verb that names a channel, and the one that also carries a message. -/
private def speaking (said : String) (text : String) : List Verb :=
  let channel : ChannelName := ⟨said⟩
  [ .invite channel "/tmp/host" .forever both
  , .join ⟨"kusanagi2:00"⟩ channel
  , .send channel text
  , .read channel
  , .readAfter channel 3
  , .revoke channel
  ]

/--
Every argv token that is the program's own word rather than the caller's.

Taken from the door rather than written down, so it cannot drift: the name
fed in here cannot appear in a command line, so whatever comes back is the
fixed vocabulary — the verbs, the flags, and the numbers beside them.

A generated name is kept clear of this set. That is not a weakening: a channel
called `read` would collide with the verb `read` and prove nothing either way,
and the question is whether the caller's word appears, not whether two
vocabularies can share a string.
-/
def fixed : List String := (speaking "\x00" "\x00").flatMap Verb.spoken

/-- Whether `needle` occurs anywhere inside `haystack`. -/
private def mentions (haystack needle : String) : Bool :=
  needle.isEmpty || (haystack.splitOn needle).length > 1

/-- A name somebody chose and a message they wrote, which is all a slip can leak. -/
structure Words where
  /-- What the caller calls the channel. -/
  said : String
  /-- What the caller wrote on it. -/
  text : String
  deriving DecidableEq, Repr, Inhabited

instance : ToString Words := ⟨fun words => s!"name {words.said}, message {words.text}"⟩

/-- The letters a name may start with: no leading `-`. -/
private def leading : List Char := "abcdefghijklmnopqrstuvwxyz0123456789".toList

/-- The letters a name may continue with. -/
private def plain : List Char := leading ++ ['-']

/-- The letters a message is made of. -/
private def spoken : List Char := "abcdefghijklmnopqrstuvwxyz .".toList

/-- Uniformly one of these characters, with a stated fallback for the empty list. -/
private def anyOf (alphabet : List Char) (fallback : Char) : Gen Char :=
  match alphabet with
  | [] => pure fallback
  | first :: rest => Gen.elements first rest

/--
Draws until the value clears `ok`, and after `attempts` refusals takes what it
has.

A rejection filter usually retries without a bound. A bound is needed here because
`Gen` has no way to report that it gave up, and the alternative — a default
substituted for a draw nobody made — would be a value the generator did not
choose. Sixteen independent draws from an alphabet of thirty-six make the
unfiltered outcome an event this suite will not see.
-/
private def satisfying (attempts : Nat) (gen : Gen α) (ok : α → Bool) : Gen α :=
  match attempts with
  | 0 => gen
  | remaining + 1 => do
    let drawn ← gen
    if ok drawn then pure drawn else satisfying remaining gen ok

/-- A name the product accepts: 1 to 32 of `a-z0-9-`, never starting with `-`. -/
private def aName : Gen String :=
  satisfying 16
    (do
      let head ← anyOf leading 'a'
      let tail ← Gen.listOf (anyOf plain 'a')
      return String.ofList (head :: tail.take 31))
    (!fixed.contains ·)

/-- Something somebody would actually send, and would not want overheard. -/
private def aMessage : Gen String :=
  satisfying 16
    (do return String.ofList (← Gen.listOfLength 12 (anyOf spoken 'a')))
    (!fixed.contains ·)

/-- A name and a message, drawn together because one property needs both. -/
def anyWords : Gen Words := do return { said := ← aName, text := ← aMessage }

/--
Shrinking keeps what the generator promised: a name that is still a name, and
neither word colliding with the door's own vocabulary.

A shrunk case that broke either promise would report a counterexample the
product never has to survive.
-/
instance : Shrinkable Words where
  shrink words :=
    let named := (shrink words.said).filterMap fun smaller =>
      if smaller.isEmpty || smaller.startsWith "-" || fixed.contains smaller then none
      else some { words with said := smaller }
    let written := (shrink words.text).filterMap fun smaller =>
      if fixed.contains smaller then none else some { words with text := smaller }
    named ++ written

/-- What a verb was fed on stdin, as text. -/
private def fed (verb : Verb) : String :=
  (String.fromUTF8? (verb.piped.getD ByteArray.empty)).getD ""

/--
What one verb gives away, as a list of reasons, empty when it gives away
nothing.
-/
private def leaked (words : Words) (verb : Verb) : List String :=
  let arguments := verb.spoken
  let complaints :=
    [ (arguments.contains words.said, "the name is on the command line")
    , (arguments.contains words.text, "the message is on the command line")
    , (!mentions (fed verb) words.said, "the name was not delivered at all") ]
  (complaints.filter (·.1)).map fun (_, why) => s!"verb: {verb.described} — {why}"

/--
No argument is the caller's name or the caller's message, and the name is
still delivered — on stdin, which only this process and its parent can read.
-/
def saysNothingIdentifying (words : Words) : Verdict :=
  match (speaking words.said words.text).flatMap (leaked words) with
  | [] => .held
  | reasons => .broke (String.intercalate "\n" reasons)

/-- The claim this module exists to make. -/
def nothingIdentifyingReachesTheCommandLine : Suite :=
  .claim "nothing identifying reaches the command line"
    (forAll 100 anyWords toString (fun words => pure (saysNothingIdentifying words))
      (mkStdGen 7))

end Kusanagi.Overheard
