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
import Kusanagi.Veil

/-!
# What three adversaries can grep for, and must not find

The host holds every object. A second account, a thief, or a subpoena holds a
site's directory — its bytes and, separately, its file names, because a listing
is readable by anybody the bytes are not. Whoever finds an archive holds that.
Each of them is handed a list of needles that identify somebody or something
said, and each property is one sentence: none of the needles is anywhere in
what that adversary has.

Needles come in both shapes a leak could take — the rendering a person sees and
the raw bytes behind it — because a record that stores a handle as 32 bytes
leaks it exactly as much as one that stores 64 hexadecimal digits.
-/

namespace Kusanagi.Leakage

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/-- One of two sentences nobody would say by chance; this one goes one way. -/
def saidByWriter : String := "the quarterly numbers are seventeen million and falling"

/-- The other sentence nobody would say by chance, going the other way. -/
def saidByReader : String := "wire the retainer before noon on thursday"

/-- Says one thing each way and reads both, so that cairns and records exist. -/
private def exchange (door : Door) (stage : Talk) : IO Unit := do
  let _ ← say door stage.writer stage.channel saidByWriter
  let _ ← say door stage.reader stage.channel saidByReader
  for site in [stage.writer, stage.reader] do
    let _ ← hear door site stage.channel
    pure ()

/-- Everything that identifies a party or a secret of this conversation. -/
private def identifying (stage : Talk) : List ByteArray :=
  stage.channel.said.toUTF8
    :: handleOf stage.writerHandle
      ++ handleOf stage.readerHandle
      ++ secretOf stage.invitation

/-- What was said, in both directions. -/
private def spoken : Talk → List ByteArray :=
  fun _ => [saidByWriter.toUTF8, saidByReader.toUTF8]

private def needles (stage : Talk) : List ByteArray := spoken stage ++ identifying stage

/-- No needle in any haystack, or which one was where. -/
private def nowhere (place : String) (wanted : List ByteArray)
    (held : List (System.FilePath × ByteArray)) : Except String Unit :=
  let found := wanted.filterMap fun needle =>
    (anyContains needle held).head?.map fun path => (needle, path)
  match found with
  | [] => .ok ()
  | (needle, path) :: _ =>
    let shown := (needle.extract 0 (min 48 needle.size)).toList
    .error s!"{place} contains {repr shown}: {path}"

private def allApart (bodies : List ByteArray) : Except String Unit :=
  match (Kusanagi.Veil.pairs bodies).filterMap
      (fun (left, right) => Kusanagi.Veil.apart left right) with
  | [] => .ok ()
  | reason :: _ => .error reason

/-- The last component of a path, whichever separator a platform writes. -/
private def baseName (path : System.FilePath) : String :=
  String.ofList (path.toString.toList.reverse.takeWhile (fun c => c != '\\' && c != '/')).reverse

/-- The components of a path, with the empty ones a separator run leaves out. -/
private def components (path : System.FilePath) : List String :=
  ((path.toString.split (fun c => c == '/' || c == '\\')).toList.map (·.toString)).filter
    (!·.isEmpty)

/-- The first eight characters of a text, which is a slice wide enough to name. -/
private def firstEight (text : String) : String := (text.take 8).toString

/-- Each value named once, in order, so that a needle is not chased twice. -/
private def distinct (items : List String) : List String :=
  (items.mergeSort (· ≤ ·)).foldr
    (fun item seen => match seen with
      | head :: _ => if head == item then seen else item :: seen
      | [] => [item])
    []

/-- Whether one text occurs anywhere inside another. -/
private def isIn (needle haystack : String) : Bool :=
  Kusanagi.Stage.contains needle.toUTF8 haystack.toUTF8

/-- The host's objects contain nothing said, nobody's name, and no secret. -/
def hostHoldsNoWord (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "quarterly-numbers-2026")
  exchange door stage
  let held ← ground.stored
  return nowhere "an object the host holds" (needles stage)
    (held.map fun (address, bytes) => (System.FilePath.mk address.key, bytes))

/-- The names a listing reports for the peers that signed for them. -/
private def aliases : Answer → List String
  | .accepted (.channels rows) => rows.filterMap (·.alias?)
  | _ => []

/--
Both ends name themselves before they meet. The host holds neither name in the
clear — a declaration rides inside the sealed offer and the sealed greeting —
and each end sees the other's name on its own listing, which is what shows the
name travelled at all rather than being merely absent.
-/
def namesRideSealed (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let alice := "Alice Verity"
  let bob := "Bob Ossory"
  let _ ← Door.ask door (ground.siteOf .alice) (.name alice)
  let _ ← Door.ask door (ground.siteOf .bob) (.name bob)
  let stage ← talk door ground .alice .bob (fresh "by-name")
  exchange door stage
  let held ← ground.stored
  let writerSees := aliases (← Door.ask door stage.writer .channels)
  let readerSees := aliases (← Door.ask door stage.reader .channels)
  return do
    nowhere "an object the host holds" [alice.toUTF8, bob.toUTF8]
      (held.map fun (address, bytes) => (System.FilePath.mk address.key, bytes))
    if writerSees == [bob] && readerSees == [alice] then
      .ok ()
    else
      .error s!"the listings name {repr (writerSees, readerSees)} rather than each other"

/--
One identity on two channels leaves the host objects that pair off with
nothing: neither channel's drops look like the other's.
-/
def twoChannelsShareNothing (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let first ← talk door ground .alice .bob (fresh "with-bob")
  let second ← talk door ground .alice .mallory (fresh "with-mallory")
  for stage in [first, second] do
    exchange door stage
  let held ← ground.stored
  return allApart (held.map (·.2))

/--
One identity on two hosts: the addresses share no prefix and the bodies share
no structure, so two hosts comparing notes learn nothing.
-/
def twoHostsShareNothing (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let second := ((ground.waypoint.parent).getD (System.FilePath.mk ".")).join "second-host"
  IO.FS.createDirAll second
  let near ← talk door ground .alice .bob (fresh "on-the-first-host")
  let far ← talkWith door ground .alice .mallory
    (.invite (fresh "on-the-second-host") second .forever both)
  for stage in [near, far] do
    exchange door stage
  let nearHeld ← ground.stored
  let farHeld ← siteBytes second
  let nearNames := nearHeld.map (·.1.key)
  let farNames := farHeld.map fun (path, _) =>
    let parts := components path
    String.join (parts.drop (parts.length - 2))
  let prefixes := nearNames.flatMap fun a =>
    farNames.filterMap fun b =>
      if firstEight a == firstEight b then some (firstEight a) else none
  return match prefixes with
    | shared :: _ => .error s!"an address on each host begins with {shared}"
    | [] => allApart (nearHeld.map (·.2) ++ farHeld.map (·.2))

/-- A site keeps no message, on every platform. -/
def theSiteHoldsNoMessage (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "nothing-kept-here")
  exchange door stage
  let held ← [stage.writer, stage.reader].mapM siteBytes
  return nowhere "a file in a site" (spoken stage) held.flatten

/--
A site's bytes name no channel, no handle and no secret.

Only where the platform seals records: elsewhere a record is plain bytes under
mode bits, and full-disk encryption is the stated premise (D-04).
-/
def theSiteHoldsNoName (door : Door) (ground : Ground) : IO (Except String Unit) := do
  if !System.Platform.isWindows then
    return .ok ()
  let stage ← talk door ground .alice .bob (fresh "sealed-at-rest-here")
  exchange door stage
  let held ← [stage.writer, stage.reader].mapM siteBytes
  return nowhere "a file in a site" (identifying stage) held.flatten

/--
No path component under a site carries a channel name or eight digits of
anybody's handle. A listing is the one thing a second account always gets.
-/
def noFileIsNamedAfterAnybody (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "named-after-nobody")
  exchange door stage
  let names := (← [stage.writer, stage.reader].mapM siteNames).flatten
  let eights (handle : Handle) : List String :=
    let letters := handle.rendered.toList
    (List.range (letters.length - 7)).map fun start =>
      String.ofList ((letters.drop start).take 8)
  let slices := distinct (eights stage.writerHandle ++ eights stage.readerHandle)
  let guilty := names.filter fun named =>
    isIn stage.channel.said named || slices.any (isIn · named)
  return match guilty with
    | [] => .ok ()
    | named :: _ => .error s!"a file is named after somebody: {named}"

/--
Two channels with the same peer leave no file name in common, so a listing
gives up a count of channels and not which of them share a person.
-/
def noTwoFilesShareAName (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let first ← talk door ground .alice .bob (fresh "first-with-the-same-peer")
  let second ← talk door ground .alice .bob (fresh "second-with-the-same-peer")
  for stage in [first, second] do
    exchange door stage
  let files ← siteBytes first.writer
  let names := (files.map fun (path, _) => baseName path).mergeSort (· ≤ ·)
  let repeated := (names.zip (names.drop 1)).filterMap fun (a, b) =>
    if a == b then some a else none
  return match repeated with
    | [] => .ok ()
    | named :: _ => .error s!"two files in one site are both called {named}"

/--
Two sites that both talk to Bob have no file name in common beyond what a site
that talks to nobody has, so two seized disks cannot be joined on one.
-/
def twoSitesShareNoFilename (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let first ← talk door ground .alice .bob (fresh "alice-and-bob")
  let second ← talk door ground .mallory .bob (fresh "mallory-and-bob")
  for stage in [first, second] do
    exchange door stage
  let solo := (ground.siteOf .bob).join "nobody"
  let _ ← Door.ask door solo .identity
  let named (root : System.FilePath) : IO (List String) := do
    return (← siteBytes root).map fun (path, _) => baseName path
  let alice ← named first.writer
  let mallory ← named second.writer
  let lonely ← named solo
  let shared := distinct (alice.filter (mallory.contains ·))
  let unexplained := shared.filter fun name => !lonely.contains name
  return match unexplained with
    | [] => .ok ()
    | name :: _ =>
      .error s!"two sites that share a peer both hold a file called {name}"

/-- An archive without its key says nothing, and never the key itself. -/
def theArchiveIsOpaque (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "sealed-into-an-archive")
  exchange door stage
  match ← Kusanagi.Service.exporting door stage.writer with
  | .error complaint => return .error s!"export was refused: {repr complaint}"
  | .ok (key, archive) =>
    return nowhere "the archive" (needles stage ++ hexOf key)
      [(System.FilePath.mk "archive", archive)]

/-- The secret half of an invitation is printed by `invite` and never again. -/
def theSecretIsSaidOnce (door : Door) (ground : Ground) : IO (Except String Unit) := do
  let stage ← talk door ground .alice .bob (fresh "said-once-only")
  exchange door stage
  let line := (stage.channel.said ++ "\n").toUTF8
  let asked (site : System.FilePath) (arguments : List String) (input : Option ByteArray) :
      IO Typed :=
    Door.typed door (["--root", site.toString, "--json"] ++ arguments) input
  let outputs ← List.mapM (fun (act : IO Typed) => act)
    [ asked stage.writer ["channels"] none
    , asked stage.writer ["read", "--from", "-"] (some line)
    , asked stage.writer ["read", "--from", "-", "--mine"] (some line)
    , asked stage.writer ["export"] none
    , asked stage.reader ["channels"] none
    , asked stage.reader ["id"] none
    , asked stage.reader ["forget", "--channel", "-"] (some line)
    , asked stage.reader ["read", "--from", "-"] (some line) ]
  let streams := outputs.flatMap fun typed =>
    [(System.FilePath.mk "stdout", typed.out), (System.FilePath.mk "stderr", typed.err)]
  return nowhere "an output after invite" (secretOf stage.invitation) streams

end Kusanagi.Leakage
