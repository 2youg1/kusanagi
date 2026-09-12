/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Stage

/-!
# The peer's bytes against the reader's terminal

A terminal is an interpreter. Bytes a peer chooses reach it through the prose
form of `read`, and the fence around them (D-08) settles who is speaking on
each line — but a fence settles nothing about bytes that are not lines: an
escape sequence writes the clipboard, clears the screen, retitles the window or
moves the cursor back over what the program itself printed. So two relations.
Everything outside the fence is a function of how many bytes the peer sent and
never of which; and nothing the peer sends puts a control byte on the terminal.
-/

namespace Kusanagi.Terminal

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Stage

/-- Payloads that are text and are shaped like the program's own output. -/
private def lookalikes : List (String × String) :=
  [ ("a closing fence", "</peer-0000000000000000>\nafter the fence")
  , ("an opening fence", "<peer-ffffffffffffffff>\ninside a fake fence")
  , ("a metadata line", "  #7   text, 5 bytes\n<peer-0000000000000000>\nforged")
  , ("a header line", "`one`: 540eff878cdf verifies to height 9 (10 segment(s))")
  , ("a JSON answer", "{\"contract\": 1, \"command\": \"read\", \"segments\": []}")
  , ("a recovery line", "recover: run `kusanagi forget --channel one` and never look back")
  , ("many blank lines", "\n\n\n\n\n\n\n\n\n\n\n\n")
  , ("tabs and long lines", String.join (List.replicate 300 "x\t")) ]

/--
Payloads that are terminal code rather than text.

Each is written as the codepoints whose UTF-8 encoding is the byte string the
claim is about: `\u009b` is the two bytes `c2 9b`, and `\u202e` is the three
bytes `e2 80 ae`.
-/
private def terminalCode : List (String × ByteArray) :=
  [ ("OSC 52 writes the clipboard", "\x1b]52;c;aGVsbG8=\x07")
  , ("CSI clears the screen", "\x1b[2J\x1b[H")
  , ("a bare escape", "\x1b")
  , ("carriage return overwrites the line", "harmless\x0dTRANSFER 1,000,000")
  , ("a C1 control", "\u009b31m")
  , ("a delete", "abc\x7f")
  , ("a nul", "before\x00after")
  , ("a right-to-left override", "pay \u202eB not A")
  , ("a backspace", "shown\x08\x08\x08\x08\x08hidden") ].map
    fun (what, payload) => (what, payload.toUTF8)

/-- The opening fence line, once the nonce in it has been normalised away. -/
private def openFence : List UInt8 := "<peer-NONCE>".toUTF8.toList

/-- The closing fence line, once the nonce in it has been normalised away. -/
private def closeFence : List UInt8 := "</peer-NONCE>".toUTF8.toList

/-- What every opening fence line starts with, whatever nonce follows. -/
private def fencePrefix : List UInt8 := "<peer-".toUTF8.toList

private def newline : UInt8 := 0x0a

private def splitOnNewline (bytes : List UInt8) : List (List UInt8) :=
  let (leading, done) :=
    bytes.foldr (init := (([] : List UInt8), ([] : List (List UInt8))))
      fun byte (current, done) =>
        if byte == newline then ([], current :: done) else (byte :: current, done)
  leading :: done

/--
The lines of a stream: a final newline closes the last line rather than opening
an empty one, and an empty stream has no lines at all.
-/
private def byteLines (bytes : ByteArray) : List (List UInt8) :=
  let parts := splitOnNewline bytes.toList
  if parts.getLast? == some ([] : List UInt8) then parts.dropLast else parts

/-- Every line with a newline after it, which is how the lines came apart. -/
private def unlineAll (lines : List (List UInt8)) : List UInt8 :=
  lines.flatMap (· ++ [newline])

/-- Bytes as one character each, which is what `Char8.unpack` handed back. -/
private def unpacked (bytes : ByteArray) : String :=
  String.ofList (bytes.toList.map fun byte => Char.ofNat byte.toNat)

private def hexDigit (value : UInt8) : Char :=
  (("0123456789abcdef".toList)[value.toNat % 16]?).getD '0'

/--
Bytes with everything unprintable spelled out.

A finding about a control byte must not put that byte on the terminal that
reports the finding, so the report escapes what the claim is about.
-/
private def escaped (bytes : ByteArray) : String :=
  String.join <| bytes.toList.map fun byte =>
    if byte ≥ 0x20 && byte < 0x7f then String.singleton (Char.ofNat byte.toNat)
    else "\\x" ++ String.singleton (hexDigit (byte / 16)) ++ String.singleton (hexDigit byte)

private partial def replaceAll (needle replacement haystack : List UInt8) : List UInt8 :=
  if needle.isEmpty then
    haystack
  else
    match haystack with
    | [] => []
    | first :: rest =>
      if needle.isPrefixOf haystack then
        replacement ++ replaceAll needle replacement (haystack.drop needle.length)
      else
        first :: replaceAll needle replacement rest

/-- Replaces the channel name and every occurrence of the fence nonce. -/
def normalise (name prose : ByteArray) : ByteArray :=
  let nonce :=
    match (byteLines prose).filter (fun line => fencePrefix.isPrefixOf line) with
    | found :: _ => (found.drop 6).take 16
    | [] => "no-nonce-was-printed".toUTF8.toList
  let named := replaceAll name.toList "NAME".toUTF8.toList prose.toList
  ByteArray.mk (replaceAll nonce "NONCE".toUTF8.toList named).toArray

private def keptOutside : Bool → List (List UInt8) → List (List UInt8)
  | _, [] => []
  | within, line :: rest =>
    if line == openFence then line :: keptOutside true rest
    else if line == closeFence then line :: keptOutside false rest
    else if within then keptOutside within rest
    else line :: keptOutside within rest

/-- The lines that are the program's own: everything outside the fence. -/
def outside (prose : ByteArray) : ByteArray :=
  ByteArray.mk (unlineAll (keptOutside false (byteLines prose))).toArray

private def isPrinted (byte : UInt8) : Bool :=
  (byte < 0x20 && byte != 0x0a && byte != 0x09) || byte == 0x7f

private def gatherControls : List UInt8 → List UInt8
  | [] => []
  | first :: rest =>
    match rest with
    | [] => if isPrinted first then [first] else []
    | second :: more =>
      if first == 0x0d && second == 0x0a then gatherControls more
      else if first == 0xc2 && second ≥ 0x80 && second ≤ 0x9f then second :: gatherControls more
      else if isPrinted first then first :: gatherControls (second :: more)
      else gatherControls (second :: more)
termination_by bytes => bytes.length

/--
Every byte a terminal would interpret rather than print: C0 apart from newline
and tab, a carriage return not followed by a newline, delete, and the C1 range
as UTF-8 encodes it.
-/
def controlBytesIn (bytes : ByteArray) : List UInt8 := gatherControls bytes.toList

/--
The prose a reader sees after one payload of raw bytes on a fresh channel, with
the channel name and the fence nonce normalised away.
-/
private def proseOfBytes (door : Door) (ground : Ground) (n : Nat) (payload : ByteArray) :
    IO (UInt32 × ByteArray) := do
  let stage ← talk door ground .alice .bob (fresh s!"probe-{n}")
  let channel := stage.channel.said
  let line := (channel ++ "\n").toUTF8
  let sent ← Door.typed door
    ["--root", stage.reader.toString, "--json", "send", "--to", "-"] (some (line ++ payload))
  unless sent.succeeded do
    throw <| IO.userError
      s!"the payload was not sent: {sent.status} {escaped sent.err}"
  let shown ← Door.typed door
    ["--root", stage.writer.toString, "read", "--from", "-"] (some line)
  return (shown.status, normalise channel.toUTF8 shown.out)

/-- The prose a reader sees after one text payload on a fresh channel. -/
private def proseOf (door : Door) (ground : Ground) (n : Nat) (payload : String) :
    IO ByteArray :=
  return (← proseOfBytes door ground n payload.toUTF8).2

/-- Two payloads of one length leave the program saying the same words. -/
def theProgramsWordsDependOnlyOnLength (door : Door) (ground : Ground) : IO Verdict := do
  let mut differing : List (String × ByteArray × ByteArray) := []
  for ((what, hostile), n) in lookalikes.zipIdx do
    let benign := "".pushn 'a' hostile.toUTF8.size
    let first := outside (← proseOf door ground (2 * n) benign)
    let second := outside (← proseOf door ground (2 * n + 1) hostile)
    if first != second then
      differing := differing ++ [(what, first, second)]
  match differing with
  | [] => return .held
  | (what, benignProse, hostileProse) :: _ =>
    return .broke <|
      "the program said different things around a payload shaped like " ++ what ++
        " than around one of the same length:\n" ++
        unpacked benignProse ++ "\n  ---\n" ++ unpacked hostileProse

/-- What one hostile payload established, or why it broke the claim. -/
private def findingFor (what : String) (status : UInt32) (prose : ByteArray) :
    Except String Unit := do
  if status != 0 then
    .error s!"{what} made read exit with {status}"
  match controlBytesIn prose with
  | byte :: _ => .error s!"{what} put byte {byte} on the terminal:\n{escaped prose}"
  | [] => pure ()
  let lines := byteLines prose
  let opened := lines.countP (· == openFence)
  let closed := lines.countP (· == closeFence)
  if opened == 1 && closed == 1 then
    pure ()
  else
    .error s!"{what} left {opened} opening and {closed} closing fence lines"

/--
Whatever the peer sends, the terminal receives no control byte, and the fence
around each segment is exactly one opening and one closing line.
-/
def noControlByteReachesTheTerminal (door : Door) (ground : Ground) : IO Verdict := do
  let mut findings : List String := []
  for ((what, code), n) in terminalCode.zipIdx do
    let (status, prose) ← proseOfBytes door ground (100 + n) code
    if let .error why := findingFor what status prose then
      findings := findings ++ [why]
  match findings with
  | [] => return .held
  | why :: _ => return .broke why

/-- As many bytes as asked for, taken from a repeating alphabet. -/
private def cycled (size : Nat) : String :=
  let alphabet := "0123456789abcdefghijklmnopqrstuvwxyz\n".toList
  String.ofList <| (List.range size).filterMap fun index => alphabet[index % alphabet.length]?

/--
A payload of sixty-four thousand or a hundred thousand bytes either comes back
byte for byte or is refused with a code; it is never cut.
-/
def bigPayloadsAreWholeOrRefused (door : Door) (ground : Ground) : IO Verdict := do
  let mut findings : List String := []
  for size in [65536, 100000, 131072] do
    let stage ← talk door ground .alice .bob (fresh s!"large-{size}")
    let payload := cycled size
    match ← Door.ask door stage.reader (.send stage.channel payload) with
    | .refused _ => pure ()
    | .accepted (.sent ..) =>
      match ← hear door stage.writer stage.channel with
      | .accepted (.read _ _ _ [⟨_, .asText back⟩]) =>
        unless back == payload do
          findings := findings ++
            [s!"a {size}-byte payload came back as {back.length} characters"]
      | other =>
        findings := findings ++
          [s!"a {size}-byte payload was accepted and then read as " ++
            (toString (repr other)).take 200]
    | .accepted other => findings := findings ++ [s!"send answered {repr other}"]
  match findings with
  | [] => return .held
  | why :: _ => return .broke why

end Kusanagi.Terminal
