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
# One identity, one name, on both sides of a conversation

An identity is a key that signs and a name that is written down, and the two
are not the same bytes: a handle is the hash of a verifying key, so that the
width of a signature scheme stops at the places a signature is checked. That
split is worth exactly as much as the agreement it preserves, and the way it
fails is quiet — one path along the door prints a name derived from a key,
another prints something it stored earlier, and the two disagree only for
people who compare them.

Nobody compares them by hand. So these are relations between traces:

* **Both ends agree.** What an endpoint answers to under `identity` is what
  its peer reads it under, in the outcome of `join` and in the outcome of
  every `read` afterwards.
* **A listing abbreviates the same name.** What `channels` shows is a prefix
  of the whole name and not a second opinion about it.

Neither says what a handle *is*, because that is the shipped code's business
and restating it here would make this a second authority. They say that
however it is computed, it is computed once.
-/

namespace Kusanagi.Naming

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground

/-- The channel both endpoints open. Names are local, so one will do. -/
private def channel : ChannelName := ⟨"peer"⟩

/-- The name an endpoint answers to. -/
private def whoAmI (door : Door) (site : System.FilePath) : IO (Except String Handle) := do
  match ← Door.ask door site .identity with
  | .accepted (.identity handle) => return .ok handle
  | other => return .error s!"{repr other}"

/-- The author of the stream this endpoint reads on its one channel. -/
private def authorSeenBy (door : Door) (site : System.FilePath) : IO (Except String Handle) := do
  match ← Door.ask door site (.read channel) with
  | .accepted (.read _ author _ _) => return .ok author
  | other => return .error s!"{repr other}"

/-- What this endpoint's listing shows for the peer of its one channel. -/
private def peerListedBy (door : Door) (site : System.FilePath) : IO (Except String String) := do
  match ← Door.ask door site .channels with
  | .accepted (.channels [summary]) =>
    return match summary.peer with
      | none => .error "a listing showed no peer after both ends had spoken"
      | some shown => .ok shown
  | other => return .error s!"{repr other}"

/-- Two names that must be one name. -/
private def same (leftName rightName : String) (left : Except String Handle) (right : Handle) :
    Except String Unit := do
  let found := (← left).rendered
  let wanted := right.rendered
  if found == wanted then
    .ok ()
  else
    .error <|
      leftName ++ " is " ++ found ++ ", and " ++ rightName ++ " is " ++ wanted ++
        "; one identity is answering to two names"

/-- A listing shows a shortened name and never a different one. -/
private def abbreviates (what : String) (shown : Except String String) (whole : Handle) :
    Except String Unit := do
  let found ← shown
  if found.isPrefixOf whole.rendered && !found.isEmpty then
    .ok ()
  else
    .error <| what ++ " shows " ++ found ++ ", which is not the start of " ++ whole.rendered

/-- Every reason, or none. -/
private def allOf (results : List (Except String Unit)) : Except String Unit :=
  match results.filterMap (fun result => match result with
    | .error reason => some reason
    | .ok _ => none) with
  | [] => .ok ()
  | reasons => .error (reasons.foldl (fun gathered reason => gathered ++ reason ++ "\n") "")

/-- A reason is a broken claim; no reason is a claim that held. -/
private def settled : Except String Unit → Verdict
  | .ok _ => .held
  | .error reason => .broke reason

/--
Every name each endpoint produces for the other is the same name.

The trace is the ordinary one: alice invites, bob joins, each says something
and reads the other. Six names come out of it and there are only two
identities, so five equalities have to hold — and a build that derived a
handle in one place and remembered a key in another would break at least one
of them without breaking any message.
-/
def bothEndsAgreeOnWhoTheOtherIs (door : Door) (ground : Ground)
    (alice bob : System.FilePath) : IO Verdict := do
  match ← whoAmI door alice, ← whoAmI door bob with
  | .ok alicesName, .ok bobsName =>
    match ← Door.ask door alice (.invite channel ground.waypoint .forever both) with
    | .accepted (.invited _ invitation _) =>
      match ← Door.ask door bob (.join invitation channel) with
      | .accepted (.joined _ bobsOwn alicesAsSeen) =>
        let _ ← Door.ask door alice (.send channel "from alice")
        let _ ← Door.ask door bob (.send channel "from bob")
        let bobsAsSeen ← authorSeenBy door alice
        let alicesAsRead ← authorSeenBy door bob
        let listedByAlice ← peerListedBy door alice
        let listedByBob ← peerListedBy door bob
        return settled <| allOf
          [ same "bob's own name" "the name bob answers to" (.ok bobsOwn) bobsName
          , same "the name bob reads alice under" "alice's own name" (.ok alicesAsSeen) alicesName
          , same "the author alice reads on bob's stream" "bob's own name" bobsAsSeen bobsName
          , same "the author bob reads on alice's stream" "alice's own name" alicesAsRead
              alicesName
          , abbreviates "alice's listing of bob" listedByAlice bobsName
          , abbreviates "bob's listing of alice" listedByBob alicesName ]
      | other => return .broke s!"the channel could not be joined: {repr other}"
    | other => return .broke s!"the invitation was refused: {repr other}"
  | .error reason, _ => return .broke s!"an endpoint could not name itself: {reason}"
  | _, .error reason => return .broke s!"an endpoint could not name itself: {reason}"

end Kusanagi.Naming
