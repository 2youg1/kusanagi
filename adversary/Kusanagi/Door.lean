/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer

/-!
# The only place that knows a binary exists

Everything this adversary learns, it learns by running the program a user runs,
with the arguments a user types, and reading the two streams a user reads.
There is no linking, no FFI, and no shared type: what cannot be reached through
this module cannot be tested here, which is the point.
-/

namespace Kusanagi.Door

open Kusanagi.Answer

/-- A binary that has already been built. -/
structure Door where
  binary : System.FilePath
  deriving Repr, Inhabited

/--
What an invitation lets its holder do.

Two independent flags rather than an ordered level, because the product has two
independent abilities and an order would invent a rule nobody wrote.
-/
structure Abilities where
  maySend : Bool
  mayRead : Bool
  deriving DecidableEq, Ord, Repr, Inhabited

def both : Abilities := { maySend := true, mayRead := true }
def sendOnly : Abilities := { maySend := true, mayRead := false }
def readOnly : Abilities := { maySend := false, mayRead := true }
def neither : Abilities := { maySend := false, mayRead := false }

/--
How long an invitation stands.

Two values, not a number: the adversary never predicts a clock, so the only
distinction it can honestly draw is between an invitation that has already
expired and one that will not expire during a test.
-/
inductive Lifetime where
  | forever
  | instantly
  deriving DecidableEq, Ord, Repr, Inhabited

def Lifetime.seconds : Lifetime → Nat
  | .forever => 3600
  | .instantly => 0

/-- One thing to ask the program to do. -/
inductive Verb where
  | identity
  | channels
  | invite (name : ChannelName) (waypoint : System.FilePath)
      (lifetime : Lifetime) (abilities : Abilities)
  /--
  An invitation on a channel that writes one drop every N seconds.

  Apart from `invite` rather than a field on it, because every existing property
  builds an on-demand channel and none of them should have to say so. A new
  constructor makes the slotted world the thing that is opted into, which is
  what it is.
  -/
  | inviteEvery (name : ChannelName) (waypoint : System.FilePath) (period : Nat)
  /--
  An invitation on a channel that deletes each drop once the peer has read it,
  and burns the key with it.
  -/
  | inviteReleasing (name : ChannelName) (waypoint : System.FilePath)
  | join (invitation : Invite) (name : ChannelName)
  /--
  What this endpoint asks to be called. The name arrives on stdin, like every
  other word that identifies somebody.
  -/
  | name (alias? : String)
  /-- Say which channels one name stands for. Members arrive on stdin. -/
  | group (name : ChannelName) (members : List ChannelName)
  /-- One sentence to every member of a group. -/
  | sendGroup (name : ChannelName) (text : String)
  /-- This endpoint's own stream on a channel, as the peer would read it. -/
  | readMine (name : ChannelName)
  | forget (name : ChannelName)
  /-- The recovery key on the first line, the archive after it. -/
  | «import» (key : String) (archive : ByteArray)
  /-- Fill this channel's current slot and look once. -/
  | tick (name : ChannelName)
  | send (name : ChannelName) (text : String)
  | read (name : ChannelName)
  | readAfter (name : ChannelName) (floor : UInt64)
  | revoke (name : ChannelName)
  deriving Inhabited

/-- Enough of a verb to name it in a failure, without reprinting a payload. -/
def Verb.described : Verb → String
  | .identity => "identity"
  | .channels => "channels"
  | .invite channel _ lifetime abilities =>
    s!"invite {channel} for {lifetime.seconds}s can send={abilities.maySend} read={abilities.mayRead}"
  | .inviteEvery channel _ period => s!"invite {channel} every {period}s"
  | .inviteReleasing channel _ => s!"invite {channel} releasing"
  | .join _ channel => s!"join as {channel}"
  | .name alias? => s!"name as {alias?}"
  | .group channel members => s!"group {channel} of {members.map ChannelName.said}"
  | .sendGroup channel _ => s!"send to group {channel}"
  | .readMine channel => s!"read mine from {channel}"
  | .forget channel => s!"forget {channel}"
  | .«import» _ archive => s!"import {archive.size} bytes"
  | .tick channel => s!"tick {channel}"
  | .send channel _ => s!"send to {channel}"
  | .read channel => s!"read from {channel}"
  | .readAfter channel floor => s!"read from {channel} after {floor}"
  | .revoke channel => s!"revoke {channel}"

/-- What a name argument says when the name itself arrives on stdin. -/
def onStdin : String := "-"

def Abilities.listed (abilities : Abilities) : String :=
  String.intercalate ","
    (([("send", abilities.maySend), ("read", abilities.mayRead)].filter (·.2)).map (·.1))

/-- The command line, which now names nobody and quotes nothing. -/
def Verb.spoken : Verb → List String
  | .identity => ["id"]
  | .channels => ["channels"]
  | .invite _ waypoint lifetime abilities =>
    ["invite", "--name", onStdin, "--waypoint", waypoint.toString,
     "--for", toString lifetime.seconds, "--can", abilities.listed]
  | .inviteEvery _ waypoint period =>
    ["invite", "--name", onStdin, "--waypoint", waypoint.toString,
     "--for", toString Lifetime.forever.seconds, "--every", toString period]
  | .inviteReleasing _ waypoint =>
    ["invite", "--name", onStdin, "--waypoint", waypoint.toString,
     "--for", toString Lifetime.forever.seconds, "--can", both.listed, "--release"]
  | .join _ _ => ["join", "--name", onStdin]
  | .name _ => ["name", "--as", onStdin]
  | .group _ _ => ["group", "--name", onStdin]
  | .sendGroup _ _ => ["send", "--to-group", onStdin]
  | .readMine _ => ["read", "--from", onStdin, "--mine"]
  | .forget _ => ["forget", "--channel", onStdin]
  | .«import» _ _ => ["import"]
  | .tick _ => ["tick", "--from", onStdin]
  | .send _ _ => ["send", "--to", onStdin]
  | .read _ => ["read", "--from", onStdin]
  | .readAfter _ floor => ["read", "--from", onStdin, "--after", toString floor]
  | .revoke _ => ["revoke", "--from", onStdin]

private def line (name : ChannelName) : ByteArray := String.toUTF8 (name.said ++ "\n")

/--
What a verb needs on stdin, which is everything that identifies anybody.

A command line is public: every account on the machine reads another process's
arguments while it runs, and the shell keeps them afterwards. The product
answered that for the invitation first, and then for its own kind — a channel
name leaks who is talking to whom on every single message, which is the
relationship graph the derived addresses exist to hide.

So `-` stands in for every name here, and the first line of stdin carries it.
**Every property in this suite runs through that path**, which is what makes a
regression in it fail a test rather than pass unnoticed.
-/
def Verb.piped : Verb → Option ByteArray
  | .identity => none
  | .channels => none
  | .invite channel .. => some (line channel)
  | .inviteEvery channel .. => some (line channel)
  | .inviteReleasing channel _ => some (line channel)
  | .join invitation channel => some (line channel ++ String.toUTF8 invitation.line)
  | .name alias? => some (String.toUTF8 (alias? ++ "\n"))
  | .group channel members => some (members.foldl (fun acc m => acc ++ line m) (line channel))
  | .sendGroup channel text => some (line channel ++ String.toUTF8 text)
  | .readMine channel => some (line channel)
  | .forget channel => some (line channel)
  | .«import» key archive => some (String.toUTF8 (key ++ "\n") ++ archive)
  | .tick channel => some (line channel)
  | .send channel text => some (line channel ++ String.toUTF8 text)
  | .read channel => some (line channel)
  | .readAfter channel _ => some (line channel)
  | .revoke channel => some (line channel)

/--
The environment the child is given, rather than the one this process happens to
have.

Some of what this program does is decided by the environment rather than by an
argument: where a site goes when nobody says, and whether a proxy stands in
front of every request. A test that cannot set the environment cannot ask about
either, and the alternative — changing this process's own environment — would
leak into every other property running beside it.

`inherit := false` makes the listed pairs the whole environment, which is how
"this machine will not say where data lives" is asked. A `none` in `changes`
removes a variable the parent had.
-/
structure Surroundings where
  inherit : Bool := true
  changes : Array (String × Option String) := #[]
  deriving Inhabited

/-- The environment this process is running in, unaltered. -/
def inherited : Surroundings := {}

/-- What a command line did, before anything decides whether that was allowed.

`ask` turns this into an `Answer` and throws when it cannot. Typing badly on
purpose needs the layer underneath: an exit code the door is not supposed to
produce is exactly what a keyboard test is looking for, and it must be able to
see one rather than crash on it.
-/
structure Typed where
  status : UInt32
  out : ByteArray
  err : ByteArray
  deriving Inhabited

def Typed.succeeded (typed : Typed) : Bool := typed.status == 0

/--
Runs the program and takes both streams as bytes.

Bytes rather than text: the recovery lines carry punctuation outside ASCII, and
a locale-decoded stream would corrupt them on the way in and then fail to parse
for a reason that has nothing to do with the product.

Both streams are drained at once. A child that fills the stderr pipe while this
process is still reading stdout would otherwise block forever, and one of the
verbs here writes an archive to stdout.
-/
private def finish (out err : IO.FS.Handle) (wait : IO UInt32) : IO Typed := do
  let reading ← IO.asTask out.readBinToEnd Task.Priority.dedicated
  let complained ← err.readBinToEnd
  let status ← wait
  let reported ← IO.ofExcept reading.get
  return { status, out := reported, err := complained }

def capture (binary : System.FilePath) (surroundings : Surroundings)
    (arguments : List String) (input : Option ByteArray) : IO Typed := do
  let shared : IO.Process.SpawnArgs := {
    cmd := binary.toString
    args := arguments.toArray
    env := surroundings.changes
    inheritEnv := surroundings.inherit
    stdout := .piped
    stderr := .piped }
  match input with
  | none =>
    let child ← IO.Process.spawn { shared with stdin := .null }
    finish child.stdout child.stderr child.wait
  | some payload =>
    let spawned ← IO.Process.spawn { shared with stdin := .piped }
    let (pipe, child) ← spawned.takeStdin
    -- A broken pipe here is the product working. Every verb reads a bounded
    -- amount of stdin and then stops, so feeding it more than the bound closes
    -- the far end mid-write — and a harness that could not survive that could
    -- not ask about the bound at all.
    try pipe.write payload; pipe.flush catch _ => pure ()
    finish child.stdout child.stderr child.wait

/--
Finds the binary, preferring what the caller was told to use.

The justfile builds it and passes the path in `KUSANAGI_BIN`, so there is one
authority for where it is; the fallbacks exist only so that a person poking at
`lake env lean` is not stopped by an environment variable.
-/
def discover : IO (Option Door) := do
  let candidates ←
    match ← IO.getEnv "KUSANAGI_BIN" with
    | some path => pure [System.FilePath.mk path]
    | none => pure <| ["debug", "release"].flatMap fun profile =>
        ["kusanagi", "kusanagi.exe"].map fun binary =>
          System.FilePath.mk s!"../target/{profile}/{binary}"
  for candidate in candidates do
    if ← candidate.pathExists then
      return some { binary := candidate }
  return none

/--
Runs the binary with exactly these arguments, and these bytes on stdin.

No `--root`, no `--json`, nothing added: what is passed here is what a person
typed or an agent spawned, character for character.
-/
def typed (door : Door) (arguments : List String) (input : Option ByteArray) : IO Typed :=
  capture door.binary inherited arguments input

/-- The same, with the environment chosen rather than taken as it stands. -/
def typedWith (door : Door) (surroundings : Surroundings) (arguments : List String)
    (input : Option ByteArray) : IO Typed :=
  capture door.binary surroundings arguments input

/--
Asks one question of one endpoint, and reads the answer.

A refusal is an answer. A stream that will not parse is not: it means the door
has changed shape, so this throws rather than reporting a green test against a
program it can no longer read.
-/
def ask (door : Door) (site : System.FilePath) (verb : Verb) : IO Answer := do
  let arguments := ["--root", site.toString, "--json"] ++ verb.spoken
  let answered ← capture door.binary inherited arguments verb.piped
  let unreadable (raw : ByteArray) (reason : String) : IO Answer :=
    throw <| IO.userError <|
      "the door answered in a shape this adversary cannot read: " ++ reason ++
      "\n  asked: " ++ verb.described ++
      "\n  said:  " ++ (String.fromUTF8? (raw.extract 0 (min raw.size 400))).getD "(not UTF-8)"
  if answered.succeeded then
    match decodeOutcome answered.out with
    | .ok outcome => return .accepted outcome
    | .error reason => unreadable answered.out reason
  else
    match decodeComplaint answered.err with
    | .ok complaint => return .refused complaint
    | .error reason => unreadable answered.err reason

end Kusanagi.Door
