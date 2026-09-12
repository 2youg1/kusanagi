/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Door

/-!
# Driving the window from outside: the automation server, its snapshots, and the widgets they describe

Everything here is plumbing shared by every claim in `Kusanagi.Glass`: how a
`native automate` command is run and read back as bytes, how a snapshot is asked
for and parsed, and how a widget is found and pressed. No claim about privacy
lives here.
-/

namespace Kusanagi.Automation

open Kusanagi.Door (Typed)

/-- The window under test, and the directories it was given. -/
structure Glass where
  dir : System.FilePath
  appData : System.FilePath
  home : System.FilePath
  /-- The site the window opens: the CLI's default root under `LOCALAPPDATA`. -/
  site : System.FilePath
  deriving Inhabited

def automationDir (glass : Glass) : System.FilePath :=
  glass.dir / ".zig-cache" / "native-sdk-automation"

/--
One command, its bytes captured whole, run in a working directory of the
caller's choosing.

Bytes rather than text: the shell tools on a Chinese Windows answer in the
console code page, which a decoder that insists on UTF-8 refuses. The working
directory is why this does not go through `Kusanagi.Door.capture`, which spawns
the shipped binary wherever this process already stands.

Both streams are drained at once, so a child that fills the standard error pipe
while this process is still reading standard output cannot block forever.
-/
def capture (directory : Option System.FilePath) (binary : System.FilePath)
    (arguments : List String) : IO Typed := do
  let child ← IO.Process.spawn {
    cmd := binary.toString
    args := arguments.toArray
    cwd := directory
    stdin := .null
    stdout := .piped
    stderr := .piped }
  let reading ← IO.asTask child.stdout.readBinToEnd Task.Priority.dedicated
  let complained ← child.stderr.readBinToEnd
  let status ← child.wait
  let reported ← IO.ofExcept reading.get
  return { status, out := reported, err := complained }

/-- One continuation byte's six payload bits, when the byte is one. -/
private def continued? (byte : UInt8) : Option Nat :=
  if byte &&& 0xc0 == 0x80 then some (byte &&& 0x3f).toNat else none

/-- Folds `width` continuation bytes at `index` into the value the lead byte started. -/
private def follow (bytes : ByteArray) (index width value : Nat) : Option Nat :=
  match width with
  | 0 => some value
  | remaining + 1 => do
    let low ← continued? (← bytes[index]?)
    follow bytes (index + 1) remaining (value * 64 + low)

/-- The shortest value each sequence length is allowed to carry. -/
private def shortest (width : Nat) : Nat :=
  match width with
  | 0 => 0
  | 1 => 0x80
  | 2 => 0x800
  | _ => 0x10000

private partial def scan (bytes : ByteArray) (index : Nat) (seen : List Char) : List Char :=
  match bytes[index]? with
  | none => seen.reverse
  | some byte =>
    let lead := byte.toNat
    let (width, seed) :=
      if lead < 0x80 then (0, lead)
      else if 0xc2 ≤ lead && lead ≤ 0xdf then (1, lead % 32)
      else if 0xe0 ≤ lead && lead ≤ 0xef then (2, lead % 16)
      else if 0xf0 ≤ lead && lead ≤ 0xf4 then (3, lead % 8)
      else (0, 0x110000)
    match follow bytes (index + 1) width seed with
    | some code =>
      if code.isValidChar && shortest width ≤ code then
        scan bytes (index + 1 + width) (Char.ofNat code :: seen)
      else
        scan bytes (index + 1) ('\uFFFD' :: seen)
    | none => scan bytes (index + 1) ('\uFFFD' :: seen)

/--
The bytes as text, with every byte that is not part of a well-formed character
replaced by U+FFFD.

Nothing read here is trusted to be UTF-8, and a stream that will not decode is
still worth showing in a failure message. Lean's own decoder answers all or
nothing, so the replacing walk below runs only when it has already refused.
-/
def lenient (raw : ByteArray) : String :=
  match String.fromUTF8? raw with
  | some text => text
  | none => String.ofList (scan raw 0 [])

/-- One `native automate` command against the running window. -/
def automate (glass : Glass) (arguments : List String) : IO (Except String String) := do
  let answered ← capture (some glass.dir) "native" ("automate" :: arguments)
  if answered.succeeded then
    return .ok (lenient answered.out)
  else
    return .error <|
      "native automate " ++ " ".intercalate arguments ++
      " failed with " ++ toString answered.status ++ ": " ++ lenient answered.err

private def snapshotFile (glass : Glass) : System.FilePath :=
  automationDir glass / "snapshot.txt"

/--
The widget tree as the automation server publishes it, after asking for a fresh
one. The first request after `wait` can race the publisher, so it is asked again
for a moment before giving up.
-/
private def again (glass : Glass) (attempts : Nat) : IO String := do
  match ← automate glass ["snapshot"] with
  | .ok _ => return lenient (← IO.FS.readBinFile (snapshotFile glass))
  | .error reason =>
    match attempts with
    | 0 => throw (IO.userError reason)
    | remaining + 1 => do
      IO.sleep 300
      again glass remaining

def snapshot (glass : Glass) : IO String := again glass 5

/-- One widget as one line of a snapshot describes it. -/
structure Widget where
  ident : String
  role : String
  name : String
  actions : List String
  deriving Inhabited

/-- The text before the first occurrence of `needle`, and the text after it. -/
private def stripInfix (needle haystack : String) : Option (String × String) :=
  match haystack.splitOn needle with
  | [] => none
  | [_] => none
  | before :: rest => some (before, needle.intercalate rest)

/-- The text up to the first `stop`, or all of it when there is no `stop`. -/
private def upTo (stop : Char) (text : String) : String :=
  String.ofList (text.toList.takeWhile (· != stop))

/-- The text from the first `stop` onwards, the `stop` included. -/
private def from? (stop : Char) (text : String) : String :=
  String.ofList (text.toList.dropWhile (· != stop))

private def field (key text : String) : Option String :=
  (stripInfix key text).map fun (_, after) => upTo ' ' after

private def quoted (key text : String) : Option String :=
  (stripInfix key text).map fun (_, after) => upTo '"' after

private def one (line : String) : Option Widget := do
  let (_, rest) ← stripInfix "widget @w1/glass-canvas#" line
  let ident := upTo ' ' rest
  let after := from? ' ' rest
  let role ← field "role=" after
  let name ← quoted "name=\"" after
  let actions :=
    match stripInfix "actions=[" after with
    | none => []
    | some (_, listed) => (upTo ']' listed).splitOn ","
  return { ident, role, name, actions }

def widgets (shown : String) : List Widget := (shown.splitOn "\n").filterMap one

/--
The first widget of a role carrying one of the names — the English and the
Chinese label of the same button, whichever language the window chose.
-/
def named (role : String) (names : List String) (found : List Widget) : Option Widget :=
  (found.filter fun widget => widget.role == role && names.contains widget.name).head?

def press (glass : Glass) (widget : Widget) : IO (Except String String) :=
  automate glass ["widget-click", "glass-canvas", widget.ident]

def setText (glass : Glass) (widget : Widget) (value : String) : IO (Except String String) :=
  automate glass ["widget-action", "glass-canvas", widget.ident, "set_text", value]

/--
What the window showed, for a failure message: every text, button and link by
name, so a red cell says what was on screen.
-/
def seen (shown : String) : String :=
  let interesting := ["text", "button", "link", "listitem"]
  let listed :=
    (widgets shown).filterMap fun widget =>
      if interesting.contains widget.role then
        some (widget.role ++ ":" ++ String.ofList (widget.name.toList.take 40))
      else
        none
  " — on screen: " ++ (repr listed).pretty

def noErrorEvent (shown : String) : Except String Unit :=
  if (stripInfix "error event=" shown).isSome then
    .error "the runtime reported an error event"
  else
    .ok ()

end Kusanagi.Automation
