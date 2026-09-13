/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground

/-!
# Two lies a host can tell without forging anything

`Kusanagi.Model` already flips a byte, which is the lie a host tells by being
broken. These are the two it can tell while every byte it holds is a byte an
endpoint really wrote and really signed, so no signature check can see them:

* **Transplant.** Serve the object from one address at another. The bytes are
  genuine and the signature verifies; only the position is a lie. This network
  answers it with the key rather than with a check, because an address derives
  the key its contents are sealed under — so the question this property really
  asks is whether that derivation is load-bearing, and it would catch the day
  somebody makes the key depend on the segment instead.

* **Vanish.** Stop serving an object. Nothing can prevent this; a store that
  will not hand bytes over is a store that will not hand bytes over. What must
  not follow is a reader believing *less* than it has already verified,
  because "she never sent the cancellation" is a lie a disappearance can tell.

Both are stated as relations. Neither says what the program should print; each
says how two runs must stand to one another.
-/

namespace Kusanagi.Lying

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground

/-- A stocked channel, and where the host put every segment of it. -/
structure Written where
  reader : System.FilePath
  channel : ChannelName
  addresses : List Drop
  deriving DecidableEq, Repr, Inhabited

/-- The channel both endpoints open. Names are local, so one will do. -/
private def peer : ChannelName := ⟨"peer"⟩

/--
Opens a channel and writes `count` segments, keeping every address.

The addresses come from the sender's own `--json`, not from listing the host's
directory. That matters: a property that learned addresses by looking at the
host would silently stop testing anything the day the host's layout changed.
-/
def writeSome (door : Door) (writer readerSite host : System.FilePath) (count : Nat) :
    IO Written := do
  let say (n : Nat) : IO Drop := do
    match ← Door.ask door writer (.send peer s!"segment {n}") with
    | .accepted (.sent _ _ address) => return address
    | other => throw <| IO.userError s!"a segment was refused: {repr other}"
  let minted ← Door.ask door writer (.invite peer host .forever both)
  let invitation ←
    match minted with
    | .accepted (.invited _ line _) => pure line
    | other => throw <| IO.userError s!"the invitation was refused: {repr other}"
  match ← Door.ask door readerSite (.join invitation peer) with
  | .accepted (.joined ..) => pure ()
  | other => throw <| IO.userError s!"the channel could not be joined: {repr other}"
  let addresses ← (List.range' 1 count).mapM say
  return { reader := readerSite, channel := peer, addresses }

/-- What a read of this reader's one channel answers. -/
private def readingOf (door : Door) (written : Written) : IO Answer :=
  Door.ask door written.reader (.read written.channel)

/-- The height a read reports, or why it did not report one. -/
private def heightOf (door : Door) (written : Written) : IO (Except String (Option UInt64)) := do
  match ← readingOf door written with
  | .accepted (.read _ _ height _) => return .ok height
  | other => return .error s!"{repr other}"

/--
Whether the first height is below the second, with an absent height counting as
lower than any reported one: a read that reports nothing has verified nothing.
-/
private def below : Option UInt64 → Option UInt64 → Bool
  | none, none => false
  | none, some _ => true
  | some _, none => false
  | some later, some earlier => later < earlier

/--
Bytes served at an address other than their own are not read as a segment.

The two addresses are both real drops of the same stream, so nothing about
the object is unusual: same author, same key length, same shape, written
minutes apart by the same endpoint. Only the height is wrong.

Success is either a refusal or a read that stops below the transplant. Both
are honest; what fails is a read that hands the caller a segment sitting at a
height its author never put it at.
-/
def transplantIsRefused (door : Door) (ground : Ground) (written : Written) : IO Verdict := do
  match written.addresses with
  | first :: second :: _ =>
    match ← heightOf door written with
    | .error reason => return .broke s!"the stream did not read back first: {reason}"
    | .ok seen =>
      ground.transplant second first
      match ← readingOf door written with
      | .refused _ => return .held
      | .accepted (.read _ _ height _) =>
        if below height seen then
          return .held
        else
          return .broke <|
            "a segment moved to another height was read as though it " ++
              "belonged there: height " ++ s!"{repr height}" ++ " after the move, " ++
              s!"{repr seen}" ++ " before it"
      | other => return .broke s!"a read answered with something else: {repr other}"
  | _ => return .held

/--
A reader that has verified a height is never talked down from it.

Read once, let the host drop everything above the floor, read again. The
second answer may fail and it may be short, but it must not report a lower
height than the reader had already checked for itself — that number is what an
agent polls from, and a host that can lower it can replay a conversation.
-/
def historyNeverShrinks (door : Door) (ground : Ground) (written : Written) : IO Verdict := do
  match written.addresses.reverse with
  | [] => return .held
  | top :: _ =>
    match ← heightOf door written with
    | .error reason => return .broke s!"the stream did not read back first: {reason}"
    | .ok seen =>
      ground.vanish top
      match ← readingOf door written with
      | .refused _ => return .held
      | .accepted (.read _ _ height _) =>
        if below height seen then
          return .broke <|
            "the host deleted one object and walked a reader back from " ++
              "height " ++ s!"{repr seen}" ++ " to " ++ s!"{repr height}" ++
              "; a reader that has verified a height must not " ++
              "believe less than it checked"
        else
          return .held
      | other => return .broke s!"a read answered with something else: {repr other}"

end Kusanagi.Lying
