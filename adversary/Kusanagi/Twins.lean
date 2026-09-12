/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Service
import Kusanagi.Stage

/-!
# Two writers who are the same author

The protocol refuses a fork by construction: one author, one height, one
address, and the host takes the first write. What that construction has to
survive is the two ways an author becomes two writers in practice — a backup
restored beside the original, and one site driven by several processes at once.
In both the reader must see one chain with no gap and no height twice, the
loser must hear a code, and the author must still be able to write afterwards.
-/

namespace Kusanagi.Twins

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/-- Runs actions at the same time and collects their results in order. -/
private def concurrently (actions : List (IO α)) : IO (List α) := do
  let boxes ← actions.mapM fun action => IO.asTask action Task.Priority.dedicated
  boxes.mapM fun box => IO.ofExcept box.get

/-- A refusal is only useful to a machine when it carries a code. -/
private def coded (complaint : Complaint) : Except String Unit :=
  if complaint.code.stable.isEmpty then .error "a refusal without a code" else .ok ()

/-- Every send either landed or was refused with a code, and nothing else. -/
private def everySendSettled (answers : List Answer) : Except String Unit :=
  answers.forM fun answer =>
    match answer with
    | .accepted (.sent ..) => .ok ()
    | .refused complaint => coded complaint
    | .accepted other => .error s!"a send answered {repr other}"

/-- How many of the answers were an accepted send. -/
private def acceptedSends (answers : List Answer) : Nat :=
  (answers.filter fun answer =>
    match answer with
    | .accepted (.sent ..) => true
    | _ => false).length

/-- Indices strictly increasing from zero, and the height the last of them. -/
private def oneChain (answer : Answer) : Except String Unit := do
  let entries ← entriesOf answer
  let indices := entries.map Entry.index
  let counted := List.range indices.length |>.map fun step => UInt64.ofNat step
  if indices == indices.mergeSort (· ≤ ·) && indices == counted then
    .ok ()
  else
    .error s!"the reader saw heights {indices}"
  match answer with
  | .accepted (.read _ _ height _) =>
    if !indices.isEmpty && height == some (UInt64.ofNat (indices.length - 1)) then
      .ok ()
    else if indices.isEmpty && height == none then
      .ok ()
    else
      .error
        s!"the reader reports height {repr height} for {indices.length} segments"
  | _ => .ok ()

/--
A site and its restored copy both write: the reader sees one chain, and
whichever twin lost is told so.
-/
def aRestoredTwinCannotFork (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "one-author-twice")
  let _ ← say door stage.writer stage.channel "before the copy"
  match ← Kusanagi.Service.exporting door stage.writer with
  | .error complaint => return .error s!"export was refused: {repr complaint}"
  | .ok (key, archive) =>
    let twin := (ground.siteOf .mallory).join "twin"
    match ← Door.ask door twin (.«import» key archive) with
    | .accepted (.imported ..) => pure ()
    | other => throw <| IO.userError s!"the archive did not restore: {repr other}"
    let answers ← concurrently
      [ Door.ask door stage.writer (.send stage.channel "from the original")
      , Door.ask door twin (.send stage.channel "from the twin") ]
    let _ ← Door.ask door stage.writer (.send stage.channel "and the original again")
    let reading ← hear door stage.reader stage.channel
    return do
      everySendSettled answers
      oneChain reading

/--
Eight sends at once from one site: one chain, no gap, no height twice, every
refusal coded, and the site still writes afterwards.
-/
def parallelSendsNeverFork (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "eight-at-once")
  let answers ← concurrently <|
    (List.range' 1 8).map fun n =>
      Door.ask door stage.writer (.send stage.channel s!"burst {n}")
  let _ ← Door.ask door stage.writer (.send stage.channel "after the burst")
  let reading ← hear door stage.reader stage.channel
  return do
    everySendSettled answers
    let entries ← entriesOf reading
    let accepted := acceptedSends answers
    let heard := entries.filterMap fun entry =>
      match entry.carried with
      | .asText text => some text
      | .asBytes _ => none
    if entries.length == accepted + 1 then
      .ok ()
    else
      .error
        s!"{accepted} sends were accepted, one more followed, and the reader heard {entries.length}"
    if heard.contains "after the burst" then
      .ok ()
    else
      .error
        "the send after the burst never reached the reader: the site's record was left wrong"
    oneChain reading

end Kusanagi.Twins
