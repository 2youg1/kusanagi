/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Automation
import Kusanagi.Door
import Kusanagi.Glass
import Kusanagi.Ground
import Kusanagi.Stage

/-!
# Two journeys through the window, end to end (H7)

A private conversation and a room, each started by a person at the sheet,
joined by Bob at his terminal, spoken in from the composer and heard back on
the next refresh.

Black box on both sides: the window is driven through the automation server and
read through its snapshot; Bob is the CLI. Neither side is linked, and what the
window draws after a refresh is the only evidence accepted.
-/

namespace Kusanagi.Journey

open Kusanagi.Answer
open Kusanagi.Automation
open Kusanagi.Door (Door)
open Kusanagi.Glass (awaiting byName prepared running)
open Kusanagi.Ground
open Kusanagi.Stage (entriesOf hear say)

/-- Whether a needle occurs anywhere in a text. -/
private def mentions (needle haystack : String) : Bool :=
  (haystack.splitOn needle).length > 1

/--
Drives the invite sheet to a minted line: name, waypoint, optionally the room
switch, then Mint; answers the `kusanagi2:` line the window draws.
-/
private def minted (glass : Glass) (ground : Ground) (channel : String) (room : Bool) :
    IO (Except String String) := do
  let first := widgets (← awaiting glass ["新邀请", "New invitation"])
  match byName ["新邀请", "New invitation"] first with
  | none =>
    return .error ("the window never showed an invite button"
      ++ seen (String.intercalate "\n" (first.map (·.name)) ++ "\n"))
  | some new =>
    let _ ← press glass new
    IO.sleep 500
    let shot ← snapshot glass
    let sheet := widgets shot
    match named "textbox" ["名字", "Name"] sheet, named "textbox" ["Waypoint"] sheet with
    | some field, some host =>
      let _ ← setText glass field channel
      let _ ← setText glass host ground.waypoint.toString
      let switched : Except String Unit ←
        if !room then
          pure (.ok ())
        else
          match byName ["建房间而不是通道:受邀的每个人都能读到彼此",
              "A room, not a channel: everybody invited reads everybody"] sheet with
          | none => pure (.error ("the invite sheet has no room switch" ++ seen shot))
          | some toggle => do
            let _ ← press glass toggle
            pure (.ok ())
      match switched with
      | .error reason => return .error reason
      | .ok () =>
        match named "button" ["生成", "Mint"] sheet with
        | none => return .error "the invite sheet has no mint button"
        | some mint =>
          let _ ← press glass mint
          IO.sleep 4000
          let later ← snapshot glass
          let after := widgets later
          match (after.filter fun widget => "kusanagi2:".isPrefixOf widget.name).map (·.name) with
          | line :: _ =>
            for done in (byName ["完成", "Done"] after).toList do
              let _ ← press glass done
            IO.sleep 500
            return .ok line
          | [] => return .error ("the window did not show a minted line" ++ seen later)
    | _, _ => return .error "the invite sheet is missing a field"

/--
Opens the row called `channel` and types `text` into the composer labelled
`composer`, then presses `button`.
-/
private def spoken (glass : Glass) (channel : String) (composer button : List String)
    (text : String) : IO (Except String Unit) := do
  let shot ← snapshot glass
  match named "listitem" [channel] (widgets shot) with
  | none => return .error ("the rail does not list " ++ channel ++ seen shot)
  | some row =>
    let _ ← press glass row
    IO.sleep 3000
    let opened ← snapshot glass
    let page := widgets opened
    match named "textbox" composer page, named "button" button page with
    | some field, some send =>
      let _ ← setText glass field text
      let _ ← press glass send
      IO.sleep 4000
      return .ok ()
    | _, _ => return .error ("the page has no composer or send button" ++ seen opened)

private def within (glass : Glass) (text : String) : Nat → IO Bool
  | 0 => return (widgets (← snapshot glass)).any fun widget => mentions text widget.name
  | attempts + 1 => do
    let drawn := widgets (← snapshot glass)
    if drawn.any (fun widget => mentions text widget.name) then
      return true
    IO.sleep 1000
    within glass text attempts

/--
Whether `text` is drawn within one poll interval and a half: the window asks
the host every twenty seconds, and nothing here hurries it.
-/
private def drawnAfterRefresh (glass : Glass) (text : String) : IO Bool :=
  within glass text 30

private def said (read : Except String (List Entry)) : List String :=
  match read with
  | .error _ => []
  | .ok entries => entries.map (·.carried.shown)

/-- The first failure among these, or nothing to report. -/
private def allOf (findings : List (Except String Unit)) : Except String Unit :=
  findings.foldl (fun soFar finding => soFar.bind fun _ => finding) (.ok ())

/--
A person mints in the window, Bob joins at his terminal, the person writes in
the composer and Bob reads it; Bob replies and the window draws it after a
refresh.
-/
def aConversationStartsInTheWindow (door : Door) (ground : Ground) : IO (Except String Unit) := do
  match ← prepared door ground with
  | .error reason => return .error reason
  | .ok glass =>
    running glass do
      match ← minted glass ground "jie" false with
      | .error reason => return .error reason
      | .ok line =>
        match ← Door.ask door (ground.siteOf .bob) (.join ⟨line⟩ ⟨"win"⟩) with
        | .accepted (.joined ..) =>
          let typed ← spoken glass "jie" ["消息", "Message"] ["发送", "Send"]
            "from the window: seventeen"
          let heard := said (entriesOf (← hear door (ground.siteOf .bob) ⟨"win"⟩))
          let _ ← say door (ground.siteOf .bob) ⟨"win"⟩ "from the terminal: eighteen"
          let drawn ← drawnAfterRefresh glass "from the terminal: eighteen"
          return allOf
            [ typed
            , if heard.contains "from the window: seventeen" then .ok ()
              else .error s!"Bob read {(repr heard).pretty}"
            , if drawn then .ok ()
              else .error "the window did not show Bob's reply after a refresh" ]
        | other =>
          return .error s!"Bob could not join with the line the window showed: {repr other}"

/--
The same journey through a room: founded at the sheet with the room switch,
joined by Bob, one line each way.
-/
def aRoomIsFoundedInTheWindow (door : Door) (ground : Ground) : IO (Except String Unit) := do
  match ← prepared door ground with
  | .error reason => return .error reason
  | .ok glass =>
    running glass do
      match ← minted glass ground "hall" true with
      | .error reason => return .error reason
      | .ok line =>
        let bob := ground.siteOf .bob
        let root := ["--root", bob.toString, "--json"]
        let joined ← Door.typed door (root ++ ["room-join", "--name", "-"])
          (some ("hall\n" ++ line).toUTF8)
        if !joined.succeeded then
          return .error s!"Bob could not join the room: {lenient joined.out}"
        else
          let typed ← spoken glass "hall" ["广播", "Broadcast"] ["发给所有人", "Send to all"]
            "room from the window"
          -- The founder's window admits Bob on its own read; Bob then hears the room.
          let _ ← drawnAfterRefresh glass "room from the window"
          let reading ← Door.typed door (root ++ ["room-read", "--name", "-"])
            (some "hall\n".toUTF8)
          let _ ← Door.typed door (root ++ ["room-send", "--name", "-"])
            (some "hall\nroom from the terminal".toUTF8)
          let drawn ← drawnAfterRefresh glass "room from the terminal"
          return allOf
            [ typed
            , if mentions "room from the window" (lenient reading.out) then .ok ()
              else .error s!"Bob's room read did not carry the line: {lenient reading.out}"
            , if drawn then .ok ()
              else .error "the window did not show Bob's room line after a refresh" ]

end Kusanagi.Journey
