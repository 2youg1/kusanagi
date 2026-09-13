/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Lean.Data.Json
import Kusanagi.Answer
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Stage

/-!
# A room: every member sweeps one ward, every member's stream is read

Two claims, and they are the two halves of the price D-17 wrote down.
A member who hands over their disk and their reads hands over every other
member's handle — that is the cost, stated so that nobody mistakes a room
for a fan-out. The host that holds every drop of the room holds no handle,
no name and no sentence — that is what the cost buys.
-/

namespace Kusanagi.Room

open Lean (Json)
open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/--
The room every property here starts from: founded by Alice, joined by Bob
and Mallory, admitted on Alice's first read, one sentence from each.
-/
structure Founded where
  handles : List (Site × Handle)
  said : List String
  deriving Inhabited

/-- The room's local name, on the first line of stdin like every other name. -/
private def room : ByteArray := String.toUTF8 "team\n"

/-- One sentence from each member, in the order the cast is listed. -/
private def sentences : List String :=
  ["alice on the ledger", "bob on the shortfall", "mallory on the auditors"]

/-- A stream as it can be quoted in a failure, whether or not it is text. -/
private def spelled (raw : ByteArray) : String :=
  (String.fromUTF8? raw).getD s!"{raw.toList}"

/-- Carries on when the door did what was asked, and throws when it refused. -/
private def accepted (what : String) (answered : Typed) : IO Unit :=
  if answered.succeeded then
    pure ()
  else
    throw <| IO.userError
      s!"{what} was refused: {spelled answered.out}{spelled answered.err}"

/-- The string at `key` of a JSON object, when the stream is one and it is there. -/
private def field (key : String) (raw : ByteArray) : Option String := do
  let text ← String.fromUTF8? raw
  let parsed ← (Json.parse text).toOption
  let value ← (parsed.getObjVal? key).toOption
  (value.getStr?).toOption

/--
Who wrote, as a room read reports it: the `author` of every thread it carried.

A thread that is not an object, or an object with no `author` string, is passed
over rather than made into a failure — the claim is about which handles a
member can name, not about the shape of a field nobody wrote a rule for.
-/
private def authors (raw : ByteArray) : Option (List String) := do
  let text ← String.fromUTF8? raw
  let parsed ← (Json.parse text).toOption
  let threads ← (parsed.getObjVal? "threads").toOption
  let listed ← (threads.getArr?).toOption
  return listed.toList.filterMap fun thread =>
    (thread.getObjVal? "author").toOption.bind (·.getStr?.toOption)

/-- The command line every verb here is typed on, at one site. -/
private def argv (ground : Ground) (site : Site) (rest : List String) : List String :=
  ["--root", (ground.siteOf site).toString, "--json"] ++ rest

/-- Mints one room invitation on Alice, and spends it at the named site. -/
private def admit (door : Door) (ground : Ground) (site : Site) : IO Unit := do
  let invited ←
    Door.typed door
      (argv ground .alice ["room-invite", "--name", "-", "--for", "3600"]) (some room)
  accepted "minting a room invitation" invited
  let line ←
    match field "invite" invited.out with
    | some line => pure line
    | none => throw <| IO.userError "the invitation carried no line"
  accepted "joining the room"
    (← Door.typed door (argv ground site ["room-join", "--name", "-"])
      (some (room ++ String.toUTF8 line)))

/-- This site's own handle, as it reports it. -/
private def handleAt (door : Door) (ground : Ground) (site : Site) : IO Handle := do
  match ← Door.ask door (ground.siteOf site) .identity with
  | .accepted (.identity handle) => return handle
  | other => throw <| IO.userError s!"no identity: {repr other}"

/-- Founds the room, admits both joiners, and has every member say one thing. -/
def assemble (door : Door) (ground : Ground) : IO Founded := do
  let founded ←
    Door.typed door
      (argv ground .alice ["room", "--name", "-", "--waypoint", ground.waypoint.toString])
      (some room)
  accepted "founding the room" founded
  for site in [Site.bob, Site.mallory] do
    admit door ground site
  -- The founder's read is what admits: it reads each introduction stream,
  -- re-signs the muster once, and carries it on her own stream.
  accepted "the founder's first read"
    (← Door.typed door (argv ground .alice ["room-read", "--name", "-"]) (some room))
  for (site, sentence) in cast.zip sentences do
    accepted "a room send"
      (← Door.typed door (argv ground site ["room-send", "--name", "-"])
        (some (room ++ String.toUTF8 sentence)))
  let named ← cast.mapM fun site => do return (site, ← handleAt door ground site)
  return { handles := named, said := sentences }

/--
Bob reads the room and can name every other member by handle. That is the
price of a room, and it is paid in the open: the reader is told who wrote.
-/
def aMemberCanListEveryMember (door : Door) (ground : Ground) : IO Verdict := do
  let squad ← assemble door ground
  let reading ←
    Door.typed door
      ["--root", (ground.siteOf .bob).toString, "--json", "room-read", "--name", "-"]
      (some room)
  match authors reading.out with
  | none => return .broke s!"bob's read did not parse as a room: {spelled reading.out}"
  | some named =>
    let owed := squad.handles.map (·.2.rendered)
    if named.mergeSort (· ≤ ·) == owed.mergeSort (· ≤ ·) then
      return .held
    else
      let holding := squad.handles.map fun (site, handle) => s!"{site}={handle}"
      return .broke s!"bob's read names {named} and the room holds {holding}"

/--
The host holds every drop of the room and can find in them no member's
handle, no room name, and no sentence said.
-/
def theHostHoldsNoMember (door : Door) (ground : Ground) : IO Verdict := do
  let squad ← assemble door ground
  let held ← ground.stored
  let objects := held.map fun (address, bytes) => (System.FilePath.mk address.key, bytes)
  let needles :=
    squad.handles.flatMap (fun member => handleOf member.2)
      ++ squad.said.map String.toUTF8
      ++ [String.toUTF8 "team"]
  let found := needles.flatMap fun needle => (anyContains needle objects).map (needle, ·)
  match found with
  | (needle, place) :: _ =>
    return .broke s!"the host can find {spelled needle} in {place.toString}"
  | [] =>
    if objects.length ≥ 3 then
      return .held
    else
      return .broke
        s!"the host holds {objects.length} objects; a room of three said three things"

end Kusanagi.Room
