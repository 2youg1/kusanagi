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
# What a site keeps, gives back, lets go of — and how little it trusts its own disk

Zero trust does not stop at the network. A site's directory is written by this
program and read by this program, and that is exactly the kind of provenance an
attacker with write access, a failing disk or a half-finished copy forges for
free. So the disk is treated like the host: every file may be wrong, and being
wrong must produce a coded refusal or the same answer, never a different answer
and never a new identity.
-/

namespace Kusanagi.Custody

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/-- A refusal that carries a stable code, which is the shape every refusal has. -/
private def coded (what : String) (complaint : Complaint) : Except String Unit :=
  if complaint.code.stable.isEmpty then
    .error s!"{what} was refused without a code"
  else
    .ok ()

/-- The first failure among the findings, or nothing to report. -/
private def allOf (findings : List (Except String Unit)) : Except String Unit :=
  findings.forM fun finding => finding

/-- Changes the byte at one offset. An offset past the end changes nothing. -/
private def flipAt (index : Nat) (bytes : ByteArray) : ByteArray :=
  if h : index < bytes.size then bytes.set index (bytes[index]'h + 1) else bytes

/--
Asks for a verb that must be refused, into a root that must stay as empty as
the refusal found it.
-/
private def refusedAndEmpty (door : Door) (what : String) (root : System.FilePath)
    (verb : Verb) : IO (Except String Unit) := do
  let answer ← Door.ask door root verb
  -- A root that was never created holds nothing, which is what an untouched
  -- root looks like from here.
  let files ← siteBytes root
  match answer with
  | .refused complaint =>
    if files.isEmpty then
      return coded what complaint
    else
      return .error s!"{what} was refused but left {files.length} file(s) behind"
  | .accepted outcome => return .error s!"{what} was accepted: {repr outcome}"

/-- Asks for a verb that must be refused, wherever it was aimed. -/
private def refusedOnly (door : Door) (what : String) (root : System.FilePath)
    (verb : Verb) : IO (Except String Unit) := do
  match ← Door.ask door root verb with
  | .refused complaint => return coded what complaint
  | .accepted outcome => return .error s!"{what} was accepted: {repr outcome}"

/--
A wrong key, a damaged archive and an occupied root are each refused, and a
refused import leaves the root as empty as it found it.
-/
def importRefusesWhatIsNotItsKey (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "to-be-archived")
  let _ ← say door stage.writer stage.channel "kept for later"
  match ← Kusanagi.Service.exporting door stage.writer with
  | .error complaint => return .error s!"export was refused: {repr complaint}"
  | .ok (key, archive) =>
    let elsewhere (name : String) : System.FilePath := (ground.siteOf .mallory).join name
    let wrongKey := key.map fun c => if c == '0' then '1' else '0'
    let damaged := flipAt (archive.size / 2) archive
    let findings := [
      ← refusedAndEmpty door "a wrong key" (elsewhere "wrong-key")
          (.«import» wrongKey archive),
      ← refusedAndEmpty door "a malformed key" (elsewhere "bad-key")
          (.«import» "not-a-key" archive),
      ← refusedAndEmpty door "a damaged archive" (elsewhere "damaged")
          (.«import» key damaged),
      ← refusedAndEmpty door "a truncated archive" (elsewhere "short")
          (.«import» key (archive.extract 0 100)),
      ← refusedOnly door "an occupied root" stage.reader (.«import» key archive)]
    return allOf findings

/--
A site restored from its archive into an empty root reads exactly what the
original read: the same entries, the same height, from the same host.
-/
def aRestoredSiteReadsTheSame (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "carried-across")
  for text in ["first", "second", "third"] do
    let _ ← say door stage.reader stage.channel text
  let before ← hear door stage.writer stage.channel
  match ← Kusanagi.Service.exporting door stage.writer with
  | .error complaint => return .error s!"export was refused: {repr complaint}"
  | .ok (key, archive) =>
    let restored := (ground.siteOf .mallory).join "restored"
    match ← Door.ask door restored (.«import» key archive) with
    | .accepted (.imported ..) =>
      let after ← hear door restored stage.channel
      if after = before then
        return .ok ()
      else
        return .error <|
          s!"the restored site read differently:\n  before: {repr before}" ++
            s!"\n  after:  {repr after}"
    | other => return .error s!"the archive did not restore: {repr other}"

/-- Forgetting a channel removes it from the site and nothing from the host. -/
def forgettingLeavesNothingBehind (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let kept ← talk door ground .alice .bob (fresh "the-one-that-stays")
  let gone ← talk door ground .alice .mallory (fresh "the-one-that-goes")
  for stage in [kept, gone] do
    let _ ← say door stage.reader stage.channel "hello"
    let _ ← hear door stage.writer stage.channel
  let filesBefore ← siteBytes kept.writer
  let objectsBefore ← ground.stored
  let forgotten ← Door.ask door kept.writer (.forget gone.channel)
  let filesAfter ← siteBytes kept.writer
  let objectsAfter ← ground.stored
  let reading ← hear door kept.writer gone.channel
  let listing ← Door.ask door kept.writer .channels
  return do
    match forgotten with
    | .accepted (.forgotten ..) => pure ()
    | other => throw s!"forget answered {repr other}"
    if filesAfter.length ≥ filesBefore.length then
      throw <|
        s!"forgetting a channel left the site with {filesAfter.length} files," ++
          s!" from {filesBefore.length}"
    if objectsAfter.map (·.1) ≠ objectsBefore.map (·.1) then
      throw "forgetting a channel changed what the host holds"
    match reading with
    | .refused complaint => coded "reading a forgotten channel" complaint
    | .accepted outcome => throw s!"a forgotten channel still reads: {repr outcome}"
    match listing with
    | .accepted (.channels summaries) =>
      if summaries.any (fun listed => decide (listed.name = gone.channel)) then
        throw s!"a forgotten channel is still listed: {repr listing}"
      else
        pure ()
    | other => throw s!"a forgotten channel is still listed: {repr other}"

/-- The endings and beginnings that name a file nobody meant to leave behind. -/
private def halfWritten (name : String) : Bool :=
  [".tmp", "~", ".part"].any (fun ending => name.endsWith ending) || name.startsWith "."

/-- After any sequence of verbs, nothing is left half-written anywhere. -/
def nothingIsLeftHalfWritten (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "finished-cleanly")
  for text in ["one", "two"] do
    let _ ← say door stage.writer stage.channel text
  let _ ← say door stage.reader stage.channel "three"
  for site in [stage.writer, stage.reader] do
    let _ ← hear door site stage.channel
  let _ ← Door.ask door stage.writer (.send (fresh "no-such-channel") "refused")
  let names := (← siteNames stage.writer) ++ (← siteNames stage.reader)
  let staging := ground.waypoint.join ".staging"
  IO.FS.createDirAll staging
  let leftovers := (← staging.readDir).toList.map (·.fileName)
  match names.filter halfWritten, leftovers with
  | [], [] => return .ok ()
  | name :: _, _ => return .error s!"a site holds a temporary file: {name}"
  | _, name :: _ => return .error s!"the host's staging area still holds {name}"

/-- The verbs that must be refused, each named the way a failure would name it. -/
private def refusals (stage : Talk) : List (String × Verb) :=
  [ ("accepting your own invitation", .join stage.invitation (fresh "own-invitation"))
  , ("sending on a channel that is not here", .send (fresh "not-here") "x")
  , ("reading a channel that is not here", .read (fresh "not-here"))
  , ("forgetting a channel that is not here", .forget (fresh "not-here"))
  , ("revoking on a channel that is not here", .revoke (fresh "not-here"))
  , ("importing into an occupied root",
      .«import» "0000000000000000000000000000000000000000000000000000000000000000"
        "x".toUTF8)
  , ("a group naming a channel that is not here", .group (fresh "team") [fresh "not-here"]) ]

/-- A verb that is refused has not touched the disk. -/
def aRefusedVerbChangesNothingOnDisk (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "left-exactly-as-found")
  let _ ← say door stage.reader stage.channel "hello"
  let _ ← hear door stage.writer stage.channel
  let alice := stage.writer
  let before ← siteBytes alice
  let findings ← (refusals stage).mapM fun (what, verb) => do
    let answer ← Door.ask door alice verb
    let after ← siteBytes alice
    match answer with
    | .accepted outcome => return (.error s!"{what} was accepted: {repr outcome}" : Except _ _)
    | .refused complaint =>
      if after != before then
        return .error s!"{what} was refused and still changed the site"
      else
        return coded what complaint
  return allOf findings

/--
Flip one byte in any one file of a site: the identity reported is the same or
the verb is refused with a code; a read is the same answer or a coded refusal.
Never a new identity, never a different history, never a crash.
-/
def aCorruptedFileNeverChangesWhoYouAre (door : Door) (ground : Ground) :
    IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "trusts-its-disk-not-at-all")
  for text in ["alpha", "beta"] do
    let _ ← say door stage.reader stage.channel text
  let alice := stage.writer
  let identity ← Door.ask door alice .identity
  let reading ← hear door alice stage.channel
  let files ← siteBytes alice
  let findings ← files.mapM fun (path, original) => do
    IO.FS.writeBinFile path (flipAt (original.size / 2) original)
    let who ← Door.ask door alice .identity
    let what ← hear door alice stage.channel
    IO.FS.writeBinFile path original
    let named := (path.fileName).getD path.toString
    return do
      match who with
      | .accepted (.identity handle) =>
        if Answer.accepted (.identity handle) = identity then
          pure ()
        else
          throw s!"with {named} damaged, id became {repr (Outcome.identity handle)}"
      | .accepted other => throw s!"with {named} damaged, id became {repr other}"
      | .refused complaint => coded s!"id with {named} damaged" complaint
      match what with
      | .refused complaint => coded s!"read with {named} damaged" complaint
      | answered =>
        if answered = reading then
          pure ()
        else
          throw s!"with {named} damaged, read answered differently: {repr answered}"
  return allOf findings

end Kusanagi.Custody
