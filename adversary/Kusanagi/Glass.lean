/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Automation
import Kusanagi.Ground
import Kusanagi.Listener
import Kusanagi.Stage

/-!
# The window, driven from outside, against what a peer can put in it

`glass` renders a peer's bytes as markdown. D-18 rules that rendering never
causes I/O: an image is drawn as its alt text, a link has nothing bound to it,
and nothing a peer sends reaches the disk, the clipboard or the network from
this machine. Those are the claims a rogue peer would test, so they are tested
the way a rogue peer would: a listener on this machine counts connections, the
automation server reports what was drawn, and the disk and clipboard are read
back afterwards. Nothing here believes "by construction".

The window is launched with `LOCALAPPDATA` and `USERPROFILE` pointed into a
throwaway directory, so the real site and the real preferences are never
touched. The whole module skips itself when the window has not been built
(`native build -Dautomation=true -Dtrace=off` in `glass/`) or the `native` CLI
is not on PATH — CI never builds the GUI (Roadmap fact 21), so this is a gate
for the machine that ships, not for the machine that merges.
-/

namespace Kusanagi.Glass

open Kusanagi.Answer
open Kusanagi.Automation
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Listener (withListener)
open Kusanagi.Stage (say contains)

/-- Whether a needle occurs anywhere in a text. -/
private def mentions (needle haystack : String) : Bool :=
  (haystack.splitOn needle).length > 1

/-- The first failure among these, or nothing to report. -/
private def allOf (findings : List (Except String Unit)) : Except String Unit :=
  findings.foldl (fun soFar finding => soFar.bind fun _ => finding) (.ok ())

/-- Whether a command of this name can be found on `PATH`, with any of the
extensions the shell would try: the `native` CLI is a program on Windows and a
shim beside it. -/
private def onPath (command : String) : IO Bool := do
  let search := System.SearchPath.parse ((← IO.getEnv "PATH").getD "")
  for directory in search do
    for extension in ["", System.FilePath.exeExtension, "cmd", "bat"] do
      let candidate :=
        if extension.isEmpty then directory / command
        else (directory / command).addExtension extension
      if ← candidate.pathExists then
        return true
  return false

/-- Where the window is, when it has been built and can be driven. -/
def available : IO (Option System.FilePath) := do
  let dir : System.FilePath := ".." / "glass"
  let built ← (dir / "zig-out" / "bin" / "glass.exe").pathExists
  if built && (← onPath "native") then
    return some (← IO.FS.realPath dir)
  return none

private def notBuilt : String :=
  "the window is not built; run `native build -Dautomation=true -Dtrace=off` in glass/"

/--
The window's home for one run, with the binary under test beside it and nothing
said yet: the site is empty until the window or a verb writes it.
-/
def prepared (door : Door) (ground : Ground) : IO (Except String Glass) := do
  match ← available with
  | none => return .error notBuilt
  | some dir =>
    -- The world's root, reached the way the window's launcher reaches it.
    let root := ground.waypoint.parent.getD ground.root
    let appdata := root / "appdata"
    let home := root / "home"
    IO.FS.createDirAll appdata
    IO.FS.createDirAll home
    -- Copied only when it differs: a running window's verb holds the file for a
    -- moment, and replacing an identical file buys nothing.
    let beside := dir / "zig-out" / "bin" / "kusanagi.exe"
    let shipped ← IO.FS.readBinFile door.binary
    let same ←
      if ← beside.pathExists then do
        let there ← IO.FS.readBinFile beside
        pure (shipped.data == there.data)
      else
        pure false
    unless same do
      IO.FS.writeBinFile beside shipped
    return .ok { dir, appData := appdata, home, site := appdata / "kusanagi" }

/--
The window's site and Bob, peered, with the window not yet running.

A throwaway home for one run, with the binary under test placed beside the
window — it runs the `kusanagi.exe` next to itself, and a stale one there once
cost half an hour. The window's site invites under the name `lin`, which is what
the rail shows; Bob joins under the name `me`.
-/
private def staged (door : Door) (ground : Ground) : IO (Except String Glass) := do
  match ← prepared door ground with
  | .error reason => return .error reason
  | .ok glass =>
    match ← Door.ask door glass.site (.invite ⟨"lin"⟩ ground.waypoint .forever both) with
    | .accepted (.invited _ invitation _) =>
      match ← Door.ask door (ground.siteOf .bob) (.join invitation ⟨"me"⟩) with
      | .accepted (.joined ..) => return .ok glass
      | other => return .error s!"Bob could not join: {repr other}"
    | other => return .error s!"the window's site could not invite: {repr other}"

private def awaitingFor (glass : Glass) (names : List String) : Nat → IO String
  | 0 => snapshot glass
  | attempts + 1 => do
    let shot ← snapshot glass
    if (widgets shot).any (fun widget => names.contains widget.name) then
      return shot
    IO.sleep 500
    awaitingFor glass names attempts

/--
Snapshots until one of `names` is drawn, or gives up after ten seconds: the
window boots by asking three verbs, and the first screen follows them.
Snapshotting the instant the automation server answers reads a window that has
not yet heard back, and blamed the rail for it.
-/
def awaiting (glass : Glass) (names : List String) : IO String :=
  awaitingFor glass names 20

/--
A widget by what it says, whatever its role: the two languages the window speaks
are both accepted.
-/
def byName (names : List String) (found : List Widget) : Option Widget :=
  (found.filter fun widget => names.contains widget.name).head?

/-- What Bob says is what the window renders. -/
private def bobSays (door : Door) (ground : Ground) (text : String) : IO Unit := do
  let _ ← say door (ground.siteOf .bob) ⟨"me"⟩ text

/-- The window just killed may still hold the automation files for a moment. -/
private def clearing (glass : Glass) : Nat → IO Unit
  | 0 => try IO.FS.removeDirAll (automationDir glass) catch _ => pure ()
  | attempts + 1 => do
    unless ← (automationDir glass).pathExists do return
    try
      IO.FS.removeDirAll (automationDir glass)
    catch _ =>
      IO.sleep 500
      clearing glass attempts

/-- Runs the window over `act`, and kills it afterwards whatever happened. -/
def running (glass : Glass) (act : IO α) : IO α := do
  let _ ← capture none "taskkill" ["/F", "/IM", "glass.exe"]
  IO.sleep 500
  clearing glass 5
  let child ← IO.Process.spawn {
    cmd := (glass.dir / "zig-out" / "bin" / "glass.exe").toString
    cwd := some glass.dir
    -- The three the window reads, laid over the environment this process has,
    -- so the real site and the real preferences are somewhere else entirely.
    env := #[("LOCALAPPDATA", some glass.appData.toString),
             ("USERPROFILE", some glass.home.toString),
             ("HOME", some glass.home.toString)]
    stdin := .null
    stdout := .piped
    stderr := .piped }
  -- This toolchain hands a child a pipe rather than a file, so both streams are
  -- drained while the window runs and written to the logs once it has stopped.
  -- Draining is not optional: a window that filled a pipe would block on it.
  let reported ← IO.asTask child.stdout.readBinToEnd Task.Priority.dedicated
  let complained ← IO.asTask child.stderr.readBinToEnd Task.Priority.dedicated
  try
    let _ ← automate glass ["wait", "--timeout-ms", "20000"]
    act
  finally
    try child.kill catch _ => pure ()
    let _ ← child.wait
    IO.FS.writeBinFile (glass.home / "glass-out.log")
      ((← IO.wait reported).toOption.getD ByteArray.empty)
    IO.FS.writeBinFile (glass.home / "glass-err.log")
      ((← IO.wait complained).toOption.getD ByteArray.empty)

/-- The window open on the channel, with the thread drawn. -/
private def opened (glass : Glass) (act : String → IO (Except String α)) :
    IO (Except String α) :=
  running glass do
    let first ← awaiting glass ["lin"]
    match named "listitem" ["lin"] (widgets first) with
    | none => return .error "the rail does not list the channel"
    | some row =>
      let _ ← press glass row
      IO.sleep 3000
      act (← snapshot glass)

/-- A body carrying a remote image and a link, neither of which is followed. -/
def aRemoteImageIsNeverFetched (door : Door) (ground : Ground) : IO (Except String Unit) :=
  withListener .blackHole fun listener => do
    match ← staged door ground with
    | .error reason => return .error reason
    | .ok glass =>
      let host := listener.locator
      bobSays door ground s!"![probe]({host}probe.png) see [this]({host}link)"
      opened glass fun shown => do
        let drawn := widgets shown
        let altShown := drawn.any fun widget =>
          widget.role == "text" && mentions "probe" widget.name
        let imageDrawn := drawn.any fun widget =>
          widget.role == "image" && mentions "probe" widget.name
        IO.sleep 2000
        let reached ← listener.connections
        if !altShown then
          return .error ("the body was not rendered at all" ++ seen shown)
        else if imageDrawn then
          return .error "the remote image was drawn as an image rather than as its alt text"
        else if reached != 0 then
          return .error s!"the window opened {reached} connection(s) to a named host"
        else
          return noErrorEvent shown

/--
A link — `http:`, `javascript:` or `file:` — has nothing bound to press, and
pressing it anyway changes nothing.
-/
def aLinkCannotBePressed (door : Door) (ground : Ground) : IO (Except String Unit) :=
  withListener .blackHole fun listener => do
    match ← staged door ground with
    | .error reason => return .error reason
    | .ok glass =>
      let host := listener.locator
      bobSays door ground
        s!"[this]({host}link) and [that](javascript:alert(1)) \
and [file](file:///C:/Windows/win.ini)"
      opened glass fun shown => do
        let links := (widgets shown).filter (·.role == "link")
        let pressable := links.filter (·.actions.contains "press")
        for link in links do
          let _ ← press glass link
        IO.sleep 2000
        let reached ← listener.connections
        let after ← snapshot glass
        if links.length < 3 then
          return .error s!"expected three links drawn, found {links.length}{seen shown}"
        else if !pressable.isEmpty then
          return .error s!"a link can be pressed: {(repr (pressable.map (·.name))).pretty}"
        else if reached != 0 then
          return .error s!"pressing a link opened {reached} connection(s)"
        else
          return noErrorEvent after

/-- Terminal bytes from a peer are drawn as hexadecimal, never as bytes. -/
def controlBytesAreShownAsHex (door : Door) (ground : Ground) : IO (Except String Unit) := do
  match ← staged door ground with
  | .error reason => return .error reason
  | .ok glass =>
    let bytes := "\x1b]52;c;aGVsbG8=\x07 clear \x1b[2J\r".toUTF8
    let spoke ← Door.typed door
      ["--root", (ground.siteOf .bob).toString, "--json", "send", "--to", "-"]
      (some ("me\n".toUTF8 ++ bytes))
    if !spoke.succeeded then
      return .error s!"the bytes were refused with exit code {spoke.status}"
    opened glass fun shown => do
      let raw := mentions "\x1b" shown || mentions "\r" (shown.replace "\r\n" "\n")
      let hex := (widgets shown).any fun widget =>
        widget.role == "text" && mentions "1b5d35323b633b" widget.name
      if raw then
        return .error "a control byte from the peer reached the widget tree"
      else if !hex then
        return .error ("the bytes were not shown as hexadecimal" ++ seen shown)
      else
        return noErrorEvent shown

/--
Drives the invite sheet to a minted invitation; answers whether the line was
shown, and the copy button when there is one.
-/
private def minting (glass : Glass) (ground : Ground) : IO (Except String Widget) := do
  let first ← awaiting glass ["新邀请", "New invitation"]
  match named "button" ["新邀请", "New invitation"] (widgets first) with
  | none => return .error "the rail has no invite button"
  | some new =>
    let _ ← press glass new
    IO.sleep 500
    let second ← snapshot glass
    let sheet := widgets second
    match named "textbox" ["名字", "Name"] sheet, named "textbox" ["Waypoint"] sheet,
        named "button" ["生成", "Mint"] sheet with
    | some channel, some host, some mint =>
      let _ ← setText glass channel "jie"
      let _ ← setText glass host ground.waypoint.toString
      let _ ← press glass mint
      IO.sleep 2000
      let third := widgets (← snapshot glass)
      if !(third.any fun widget => "kusanagi2:".isPrefixOf widget.name) then
        return .error "the window did not show the invitation it minted"
      else
        match named "button" ["复制邀请", "Copy the invitation"] third with
        | none => return .error "the invitation has no copy button"
        | some copy => return .ok copy
    | _, _, _ => return .error ("the invite sheet is missing a field" ++ seen second)

private partial def walk (directory : System.FilePath) : IO (List System.FilePath) := do
  if !(← directory.isDir) then
    return []
  let mut found := []
  for entry in ← directory.readDir do
    found := found ++ (← if ← entry.path.isDir then walk entry.path else pure [entry.path])
  return found

/--
Every file under the throwaway home and app-data directories, except the site
the CLI keeps under `LOCALAPPDATA/kusanagi`.
-/
private def filesOutsideTheSite (glass : Glass) : IO (List System.FilePath) := do
  let fromHome ← walk glass.home
  let fromAppData ← walk glass.appData
  return (fromHome ++ fromAppData).filter fun path =>
    !glass.site.toString.isPrefixOf path.toString

/--
After a session, the disk outside the site holds nothing of the peer.

The site itself (`LOCALAPPDATA/kusanagi`) is the CLI's, and H5 answers for its
bytes. Everything else the window or its runtime may write is listed here: two
preferences of ours, the runtime's window geometry, and a runtime event log only
when the window was built with tracing on — the shipped build is not
(`-Dtrace=off`). A file not on the list is a finding.
-/
def theDiskHoldsNoPeer (door : Door) (ground : Ground) : IO (Except String Unit) := do
  match ← staged door ground with
  | .error reason => return .error reason
  | .ok glass =>
    bobSays door ground "pineapple-on-pizza-7731"
    let driven ← opened glass fun _ => minting glass ground
    let files ← filesOutsideTheSite glass
    let judged ← files.mapM fun path => do
      let bytes ← IO.FS.readBinFile path
      let needles := ["pineapple-on-pizza-7731", "kusanagi2:", ground.waypoint.toString]
      let leaked := needles.filter fun needle => contains needle.toUTF8 bytes
      let known :=
        ["kusanagi-glass.language", "kusanagi-glass.font", "windows.zon",
         "native-sdk.jsonl", "last-panic.txt", "glass-out.log",
         "glass-err.log"].contains (path.fileName.getD "")
      if !known then
        return .error s!"the window wrote a file nobody listed: {path}"
      else if leaked.isEmpty then
        return .ok ()
      else
        return .error s!"{path} holds {(repr leaked).pretty}"
    return driven.bind fun _ => allOf judged

private def powershell (command : String) : IO String := do
  let answered ← capture none "powershell" ["-NoProfile", "-Command", command]
  return lenient answered.out

/--
Nothing reaches the clipboard until a person presses copy, and then the window
says what the clipboard is.
-/
def theClipboardWaitsForAHand (door : Door) (ground : Ground) : IO (Except String Unit) := do
  match ← staged door ground with
  | .error reason => return .error reason
  | .ok glass =>
    let sentinel := s!"sentinel-{ground.waypoint.toString.length}"
    let _ ← powershell s!"Set-Clipboard -Value '{sentinel}'"
    running glass do
      match ← minting glass ground with
      | .error reason => return .error reason
      | .ok copy =>
        let untouched ← powershell "Get-Clipboard -Raw"
        if untouched.trimAscii.copy != sentinel then
          return .error <| "the clipboard changed before anybody pressed copy: "
            ++ String.ofList (untouched.toList.take 24)
        else
          let _ ← press glass copy
          IO.sleep 800
          let copied ← powershell "Get-Clipboard -Raw"
          let after ← snapshot glass
          let warned := (widgets after).any fun widget =>
            widget.role == "text" &&
              (mentions "剪贴板" widget.name || mentions "clipboard" widget.name)
          if !("kusanagi2:".isPrefixOf copied.trimAscii.copy) then
            return .error "pressing copy did not put the invitation on the clipboard"
          else if !warned then
            return .error "the window copied without saying what the clipboard is"
          else
            return noErrorEvent after

end Kusanagi.Glass
