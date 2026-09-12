/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door

/-!
# How a person actually reaches this program, and whether what it says back can be acted on

Everything else in this adversary walks verbs. Nobody types a verb: they type a
line, and they mistype it — a missed key, two letters swapped, caps lock still
on, half an invitation because the paste was cut, a shell that left its quotes
behind. An agent does something else again: it pipes bytes it did not choose the
shape of.

The four properties here are relations, never expected outputs. They say that
the two doors agree, that advice can be followed, that advice is about what was
actually supplied, and that bytes survive the trip.
-/

namespace Kusanagi.Keyboard

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door

/--
A world that already exists, described the way a keyboard sees it.

Not the model's `World`: this one holds strings, because what a person types is
a string and the point of this module is to mistype exactly those.
-/
structure Bench where
  site : System.FilePath
  other : System.FilePath
  waypoint : System.FilePath
  channel : String
  invitation : String
  deriving Inhabited

/-- One way a hand goes wrong, with a name a counterexample can print. -/
structure Slip where
  name : String
  hit : String → String

instance : ToString Slip := ⟨Slip.name⟩

/-- A command line as it was meant, and as it came out. -/
structure Typing where
  intended : List String
  keyed : List String
  slipped : Slip

private def missedKey : Slip := ⟨"a missed key", (·.drop 1 |>.toString)⟩

private def swapped (word : String) : String :=
  match word.toList with
  | first :: second :: rest => String.ofList (second :: first :: rest)
  | _ => word

private def doubled (word : String) : String :=
  match word.toList with
  | letter :: rest => String.ofList (letter :: letter :: rest)
  | _ => word

/--
The mistakes a keyboard actually makes.

Each is one physical event, not a category of malformedness: a key missed, two
fingers in the wrong order, caps lock, a paste that took half, a shell that kept
its quotes, a dash that did not register.
-/
def slips : List Slip :=
  [ missedKey
  , ⟨"a dropped last key", fun word => (word.take (word.length - 1)).toString⟩
  , ⟨"two letters swapped", swapped⟩
  , ⟨"caps lock", fun word => String.ofList (word.toList.map Char.toUpper)⟩
  , ⟨"a trailing space", (· ++ " ")⟩
  , ⟨"a leading space", (" " ++ ·)⟩
  , ⟨"quotes the shell kept", fun word => "\"" ++ word ++ "\""⟩
  , ⟨"half a paste", fun word => (word.take (max 1 (word.length / 2))).toString⟩
  , ⟨"a dash that did not register", fun word =>
      if word.startsWith "-" then (word.drop 1).toString else word⟩
  , ⟨"a doubled key", doubled⟩
  ]

/--
Which line, which slip, which token — three numbers and nothing else.

Generation happens before there is a world to type at: the ground is built in
IO, and a generator that needed it could not shrink. So what is generated is the
*choice*, and `typingOf` applies it once the bench exists.
-/
structure Choice where
  line : Nat
  slip : Nat
  token : Nat
  deriving DecidableEq, Repr, Inhabited

instance : ToString Choice :=
  ⟨fun choice => s!"line {choice.line}, slip {choice.slip}, token {choice.token}"⟩

/-- Three indices, each drawn far wider than the list it will be taken modulo. -/
def anyChoice : Gen Choice := do
  let index := Gen.choose 0 999
  return { line := ← index, slip := ← index, token := ← index }

instance : Shrinkable Choice where
  shrink choice :=
    (shrink choice.line).map (fun line => { choice with line }) ++
      (shrink choice.slip).map (fun slip => { choice with slip }) ++
      (shrink choice.token).map (fun token => { choice with token })

/--
The command lines that are worth mistyping, which are the ones people copy.

No invitation on the `join` line: the product takes it on stdin, and this bench
gives it none, so the refusal that follows is the one a person gets when they
forget to pipe.
-/
private def linesOf (bench : Bench) : List (List String) :=
  [ ["--root", bench.site.toString, "id"]
  , ["--root", bench.site.toString, "channels"]
  , ["--root", bench.site.toString, "read", "--from", bench.channel]
  , ["--root", bench.site.toString, "read", "--from", bench.channel, "--mine"]
  , ["--root", bench.site.toString, "read", "--from", bench.channel, "--after", "0"]
  , ["--root", bench.site.toString, "send", "--to", bench.channel, "a line of text"]
  , ["--root", bench.site.toString, "revoke", "--from", bench.channel]
  , ["--root", bench.site.toString, "forget", "--channel", bench.channel]
  , ["--root", bench.site.toString, "doctor", bench.waypoint.toString]
  , ["--root", bench.other.toString, "join", "--name", "someone"]
  , ["--root", bench.other.toString, "invite", "--name", "carol",
     "--waypoint", bench.waypoint.toString]
  , ["--root", bench.other.toString, "invite", "--name", "carol",
     "--waypoint", bench.waypoint.toString, "--can", "read"]
  ]

/--
A command somebody meant to type, with one slip in it.

The command lines are the ones in `README.md` and `docs/joining.md`, because
those are the ones people copy. The slip lands on any token, including the verb
and the flags — a person mistypes those as readily as a value.
-/
def typingOf (bench : Bench) (choice : Choice) : Typing :=
  let available := linesOf bench
  let line := (available[choice.line % available.length]?).getD []
  let slip := (slips[choice.slip % slips.length]?).getD missedKey
  let spot := choice.token % line.length
  { intended := line
    keyed := line.zipIdx.map fun (word, index) => if index == spot then slip.hit word else word
    slipped := slip }

/--
A world with two endpoints, one channel and one spent invitation in it.

Built once per test case rather than once per run, because following advice is
allowed to change things — `forget` really forgets — and a property whose
earlier cases decide its later ones is not a property.
-/
def prepare (door : Door) (alice bob host : System.FilePath) : IO Bench := do
  let root (site : System.FilePath) : List String := ["--root", site.toString, "--json"]
  let minted ← Door.typed door
    (root alice ++ ["invite", "--name", "bob", "--waypoint", host.toString]) none
  let invitation ←
    match decodeOutcome minted.out with
    | .ok (.invited _ offered _) => pure offered.line
    | .ok other => throw <| IO.userError s!"the bench could not be built: {repr other}"
    | .error reason => throw <| IO.userError s!"the bench could not be built: {reason}"
  let _ ← Door.typed door (root bob ++ ["join", "--name", "alice"]) (some invitation.toUTF8)
  let _ ← Door.typed door (root alice ++ ["send", "--to", "bob", "a first thing"]) none
  let _ ← Door.typed door (root alice ++ ["read", "--from", "bob"]) none
  return { site := alice, other := bob, waypoint := host, channel := "bob", invitation }

/-- Whether `needle` occurs anywhere inside `haystack`. -/
private def mentions (haystack needle : String) : Bool := (haystack.splitOn needle).length > 1

/-- As much of a stream as is worth printing beside a counterexample. -/
private def glimpse (raw : ByteArray) : String :=
  (String.fromUTF8? (raw.extract 0 (min raw.size 300))).getD "(not UTF-8)"

/--
Both doors say the same thing, and each says one of the two things it may.

A command either worked — exit 0, and stdout is an outcome — or it did not —
exit 1, and stderr is a complaint carrying a stable code and a way out. Any
third shape is a hole in the door: an exit code nobody documented, a refusal
with nothing machine-readable in it, or a code that is not a code.
-/
def shapeIsAnswerable (door : Door) (arguments : List String) :
    IO (Except String Answer) := do
  let spoke ← Door.typed door (arguments ++ ["--json"]) none
  if spoke.status == 0 then
    match decodeOutcome spoke.out with
    | .ok outcome => return .ok (.accepted outcome)
    | .error reason => return .error s!"it succeeded but stdout is not an outcome: {reason}"
  else if spoke.status == 1 then
    match decodeComplaint spoke.err with
    | .error reason =>
      return .error
        s!"it refused, but stderr is not a complaint: {reason}\n  said: {glimpse spoke.err}"
    | .ok complaint =>
      if complaint.code.stable.isEmpty then
        return .error "the complaint carries an empty code"
      else if !mentions complaint.code.stable "." then
        return .error s!"the code is not namespaced: {complaint.code}"
      else if complaint.recover.trimAscii.isEmpty then
        return .error s!"`{complaint.code}` carries no way out"
      else
        return .ok (.refused complaint)
  else
    return .error
      s!"it left with exit code {spoke.status}, which is neither success nor a refusal \
this door defines\n  said: {glimpse spoke.err}"

/--
The commands a recovery line names, as argv, with `kusanagi` dropped.

Recovery text is written for a person, so a command appears inside it as prose:
in backticks, or after a pipe, or at the end of a sentence. What is taken is
everything from `kusanagi` up to the next backtick, comma, or end of line, which
is how a person reads it too.
-/
def commandsIn (recover : String) : List (List String) :=
  ((recover.splitOn "kusanagi ").drop 1).map fun rest =>
    (List.splitOnP Char.isWhitespace
        (rest.toList.takeWhile fun letter =>
          letter != '`' && letter != ',' && letter != '\n')).filterMap fun word =>
      if word.isEmpty then none else some (String.ofList word)

/--
A word the reader is meant to fill in rather than type: `<angle brackets>` or a
SHOUTED word.
-/
private def placeholder (word : String) : Bool :=
  word.startsWith "<" ||
    (word.toList.filter (· != '-')).all fun letter => letter.isUpper || letter == '_'

/--
The verb slot of a template command names a verb this program has.

`kusanagi <VERB> --help` is a sentence about the program, not a command to run:
the verb slot is marked as the reader's to fill in, and marking it is the whole
of what this property asks for.
-/
private def verbExists (door : Door) : List String → IO (Except String Unit)
  | [] => return .error "the recovery names `kusanagi` with no verb after it"
  | verb :: _ => do
    if placeholder verb then
      return .ok ()
    let spoke ← Door.typed door [verb, "--help"] none
    if spoke.succeeded then
      return .ok ()
    else
      return .error s!"the recovery names a verb this program does not have: {verb}"

/--
A concrete command from a recovery line is run, and is allowed to fail.

Following advice may fail: `kusanagi read --from N` on a channel whose peer is
gone is still the right thing to have been told. What it may not do is fail to
*parse* — that is advice nobody can take — and it may not leave by an exit this
door does not define. Success is not inspected further, because `--help` is a
document rather than an outcome.
-/
private def runs (door : Door) (site : System.FilePath) (command : List String) :
    IO (Except String Unit) := do
  let spoke ← Door.typed door (["--root", site.toString, "--json"] ++ command) none
  if spoke.status == 0 then
    return .ok ()
  else if spoke.status == 1 then
    match decodeComplaint spoke.err with
    | .error reason =>
      return .error s!"following the advice produced an unreadable refusal: {reason}"
    | .ok followed =>
      if followed.code.stable == "kusanagi.argument" then
        return .error
          s!"the advice does not parse: `kusanagi {" ".intercalate command}` answers \
{followed.message}"
      else
        return .ok ()
  else
    return .error s!"following the advice left with exit code {spoke.status}"

/--
Every command a refusal names is a command this program admits.

A concrete one is run: it may fail for any reason except being unreadable —
`kusanagi.argument` means the advice itself does not parse, which is advice
nobody can take. A template one, marked by `<angle brackets>` or a SHOUTED word,
cannot be run literally, so what is checked is that its verb exists.
-/
def adviceIsExecutable (door : Door) (site : System.FilePath) (complaint : Complaint) :
    IO (Except String Unit) := do
  for command in commandsIn complaint.recover do
    let settled ←
      if command.any placeholder then verbExists door command else runs door site command
    match settled with
    | .error reason => return .error reason
    | .ok _ => pure ()
  return .ok ()

/--
Advice about an invitation is only given to somebody who supplied one.

The failure this rules out is specific and was real: mistype a channel name and
be told to paste the whole `kusanagi2:` line, which sends a confused person to
look for a thing they never had.

"Supplied one" is judged by the position the caller was standing in, not by
whether the text still contains the prefix. This adversary found the reason: it
dropped the leading `k` from an otherwise perfect invitation, and a rule that
searched for `kusanagi2:` concluded that no invitation had been offered — so it
called correct advice a defect. A mangled invitation is an invitation supplied,
and `join` is the one verb whose positional argument is one.
-/
def adviceIsAboutWhatWasGiven (arguments : List String) (complaint : Complaint) :
    Except String Unit :=
  let mentionsInvitation := mentions complaint.recover "kusanagi2:"
  let suppliedInvitation :=
    arguments.contains "join" || arguments.any (mentions · "kusanagi2:")
  if mentionsInvitation && !suppliedInvitation then
    .error
      s!"the advice is about an invitation, and no invitation was supplied: {complaint.recover}"
  else
    .ok ()

/-- One byte of the door's lowercase hexadecimal rendering. -/
private def digit (value : Nat) : Char := (("0123456789abcdef".toList)[value]?).getD '0'

private def wire (payload : ByteArray) : String :=
  String.ofList <| payload.toList.flatMap fun byte =>
    [digit (byte.toNat / 16), digit (byte.toNat % 16)]

/--
Whether a character may appear in something the door calls text.

Text is narrower than valid UTF-8 (door-SPEC §10): a control character other
than tab, newline and the return of a `\r\n` pair, or a bidirectional override,
makes the whole payload not text.
-/
private def allowed (letter : Char) : Bool :=
  if letter == '\t' || letter == '\n' then true
  else if letter.val ≥ 0x202A && letter.val ≤ 0x202E then false
  else if letter.val ≥ 0x2066 && letter.val ≤ 0x2069 then false
  -- The Unicode Cc category is exactly these two ranges.
  else !(letter.val < 0x20 || (letter.val ≥ 0x7f && letter.val ≤ 0x9f))

private def inert : List Char → Bool
  | '\r' :: '\n' :: rest => inert rest
  | letter :: rest => allowed letter && inert rest
  | [] => true

/--
The door reports text when every byte of it is text and hexadecimal when they
are not. Which one appears is a fact about the bytes, so what to expect is
derived the same way rather than fixed to one.
-/
private def rendered (payload : ByteArray) : Carried :=
  match String.fromUTF8? payload with
  | some said => if inert said.toList then .asText said else .asBytes (wire payload)
  | none => .asBytes (wire payload)

/--
What an agent pipes in comes back out, byte for byte.

`text` in the same record is lossy and says so; `payload` is the field a caller
parses, and this is the only place its promise is tested against bytes nobody
chose by hand.
-/
def bytesSurviveTheTrip (door : Door) (site : System.FilePath) (channel : String)
    (payload : ByteArray) : IO (Except String Unit) := do
  let spoke ← Door.typed door
    ["--root", site.toString, "--json", "send", "--to", channel] (some payload)
  if !spoke.succeeded then
    return .error s!"the payload was refused with exit code {spoke.status}"
  let heard ← Door.typed door
    ["--root", site.toString, "--json", "read", "--from", channel, "--mine"] none
  let sent := rendered payload
  match decodeOutcome heard.out with
  | .error reason => return .error s!"reading it back failed: {reason}"
  | .ok (.read _ _ _ entries) =>
    match entries.getLast? with
    | none => return .error "the segment was accepted and then not there"
    | some latest =>
      if latest.carried = sent then
        return .ok ()
      else
        return .error
          s!"what came back is not what went in:\n  in:  {repr sent}\n  \
out: {repr latest.carried}"
  | .ok other => return .error s!"reading it back answered {repr other}"

end Kusanagi.Keyboard
