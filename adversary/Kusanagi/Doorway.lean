/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground

/-!
# The edges of the command line itself, asked from outside the program

`Kusanagi.Keyboard` asks what happens when somebody types the wrong thing.
This asks about the four places where the shape of the door is decided by
something other than the characters typed: what arrives on the pipe, what clap
does with a line it cannot parse, what an argument the verb cannot act on
produces, and where a site lands when nobody says.

**These claims used to be Rust integration tests, and that was the wrong room.**
A Rust test links the library, so nothing stops one of these from reaching past
the command line into a function — and the day it does, it is still called a
test of the door. Out here there is no library to reach: one subprocess, two
streams, one exit code. `just boxes` is the gate that keeps it that way.
-/

namespace Kusanagi.Doorway

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground

/-- Whether `needle` occurs anywhere inside `haystack`. -/
private def mentions (haystack needle : String) : Bool := (haystack.splitOn needle).length > 1

/-- A condition, with the sentence to print when it does not hold. -/
private def demand (holds : Bool) (reason : String) : Except String Unit :=
  if holds then .ok () else .error reason

/-- A reason is a broken claim; no reason is a claim that held. -/
private def settled : Except String Unit → Verdict
  | .ok _ => .held
  | .error reason => .broke reason

/-- Runs one command that is expected to be refused, and hands back the refusal. -/
private def refusal (door : Door) (ground : Ground) (arguments : List String)
    (fed : ByteArray) : IO (Except String Complaint) := do
  let answered ←
    Door.typed door
      (["--root", (ground.siteOf .bob).toString, "--json"] ++ arguments) (some fed)
  if answered.succeeded then
    return .error s!"this was supposed to be refused: {arguments}"
  match decodeComplaint answered.err with
  | .error reason =>
    return .error s!"a refusal a program cannot parse is not an answer: {reason}"
  | .ok complaint => return .ok complaint

/--
This process's environment with `LOCALAPPDATA` set to something, or removed.

The Haskell version listed the inherited environment and handed the child the
whole list back with one pair edited. `Kusanagi.Door.Surroundings` says the same
thing without reading the environment first: inherit it, and change the one
variable this claim is about.
-/
private def withProfile (chosen : Option System.FilePath) : Surroundings :=
  { changes := #[("LOCALAPPDATA", chosen.map System.FilePath.toString)] }

/--
Every way a clipboard mangles a line, and none of them may lose it.

The invitation stopped being an argument because an argument is public. That
moved it onto a pipe and created a new set of edges instead: a paste with a
trailing newline, one with none, one from a Windows clipboard carrying `\r\n`,
and one a chat window padded with spaces and blank lines.

One invitation admits exactly one endpoint, so each clipboard gets its own.
-/
def invitationSurvivesAnyClipboard (door : Door) (ground : Ground) : IO Verdict := do
  let clipboards : List (String → String) :=
    [ id
    , (· ++ "\n")
    , (· ++ "\r\n")
    , fun line => "  " ++ line ++ "  \n\n" ]
  for (paste, round) in clipboards.zipIdx do
    let channel : ChannelName := ⟨s!"bob-{round}"⟩
    let inviter := ground.siteOf .alice
    let joiner := System.FilePath.mk s!"{ground.waypoint}-joiner-{round}"
    match ← Door.ask door inviter (.invite channel ground.waypoint .forever both) with
    | .accepted (.invited _ invitation _) =>
      match ← Door.ask door joiner (.join ⟨paste invitation.line⟩ ⟨"alice"⟩) with
      | .accepted (.joined ..) => pure ()
      | other => return .broke s!"clipboard {round} did not join: {repr other}"
    | other => return .broke s!"the invitation was refused: {repr other}"
  return .held

/--
Somebody typed the command and forgot the pipe.

The answer has to be the ordinary shape — a stable code and a way out — rather
than a wait. And the way out has to name the pipe, because there is no other way
in: advice that says "copy the invitation" without saying where to put it sends
a person looking for a flag this program does not have.
-/
def anEmptyPipeIsAnswered (door : Door) (ground : Ground) : IO Verdict := do
  let said ← refusal door ground ["join", "--name", "alice"] ByteArray.empty
  return settled do
    let complaint ← said
    if complaint.code != ⟨"kusanagi.malformed"⟩ then
      .error s!"an empty pipe was answered with {complaint.code}"
    else
      let way := complaint.recover
      if mentions way "pipe" && mentions way "join" then
        .ok ()
      else
        .error s!"the way out of an empty pipe does not mention it: {way}"

/--
Far more than an invitation can be.

The bound inside the program decides this rather than the parser, and what
matters is that it ends at all: a door that buffers whatever arrives has handed
the caller a way to spend this process's memory.
-/
def aFloodOnStdinIsAnswered (door : Door) (ground : Ground) : IO Verdict := do
  let flood := String.toUTF8 ("".pushn 'x' 1000000)
  let said ← refusal door ground ["join", "--name", "alice"] flood
  return settled do
    let complaint ← said
    demand (!complaint.code.stable.isEmpty) "a flood on stdin was answered with no code"

/--
An ability nobody defined, offered to a verb that takes a list of them.

Refused rather than ignored, and the way out has to name what to pass instead:
an invitation that silently granted less than it was asked for is discovered by
the person it was given to, days later, as a failure they cannot explain.
-/
def anArgumentTheVerbCannotActOnIsRefused (door : Door) (ground : Ground) : IO Verdict := do
  let said ←
    refusal door ground
      ["invite", "--name", "bob", "--waypoint", ground.waypoint.toString, "--can", "send,fly"]
      ByteArray.empty
  return settled do
    let complaint ← said
    if complaint.code != ⟨"kusanagi.argument"⟩ then
      .error s!"an unknown ability was answered with {complaint.code}"
    else
      demand (mentions complaint.recover "send")
        "the recovery does not say what to pass instead"

/--
One missed key on a flag, which is not this program's error path at all.

Found by this adversary once already: `-root` for `--root` reached clap's own
reporting, which exits with a code this door does not define and prints prose
even when the caller asked for JSON. An agent cannot act on that. Help is the
other half of the same claim — it is what a person asks for, so it succeeds and
goes to stdout.
-/
def aMistypedFlagLeavesByTheSameDoor (door : Door) (ground : Ground) : IO Verdict := do
  let refused ←
    Door.typed door ["--json", "-root", (ground.siteOf .alice).toString, "id"] none
  let asked ← Door.typed door ["--help"] none
  return settled do
    demand (refused.status == 1) s!"a mistyped flag exited {refused.status}"
    demand (refused.out.isEmpty) "a refusal put something on stdout"
    let complaint ←
      match decodeComplaint refused.err with
      | .error reason =>
        .error s!"a refusal a program cannot parse is not an answer: {reason}"
      | .ok complaint => .ok complaint
    demand (complaint.code == ⟨"kusanagi.argument"⟩)
      s!"a mistyped flag was answered with {complaint.code}"
    demand (!complaint.recover.isEmpty) "the refusal carries no way out"
    demand asked.succeeded "--help failed"
    demand (mentions ((String.fromUTF8? asked.out).getD "") "forget")
      "--help does not list the verbs"

/--
Where a site lands when nobody says where.

A relative default put an identity, every channel key and every cairn in
whatever directory the program happened to be started from — for an agent, the
repository it is editing or a folder a sync client uploads. Nothing about that
is visible at the moment it happens.

Windows only, and that is the honest limit: the program compiles one branch per
platform and this machine can only run one of them.
-/
def aSiteNobodyPlacedLandsUnderTheProfile (door : Door) (ground : Ground) : IO Verdict := do
  if !System.Platform.isWindows then
    return .skipped "this machine does not run the platform branch that decides the default"
  let profile := System.FilePath.mk s!"{ground.waypoint}-profile"
  let answered ← Door.typedWith door (withProfile (some profile)) ["--json", "id"] none
  let landed ← (profile.join "kusanagi").isDir
  let here ← (System.FilePath.mk ".").readDir
  return settled do
    demand answered.succeeded "`id` failed with a profile set"
    demand landed "no site appeared under the profile directory"
    demand (!here.any (·.fileName == ".kusanagi"))
      "a site appeared in the current directory, which is what the default was moved away from"

/-- A machine that will not say where data lives is asked rather than guessed. -/
def aMachineThatWillNotSayIsAsked (door : Door) (_ground : Ground) : IO Verdict := do
  if !System.Platform.isWindows then
    return .skipped "this machine does not run the platform branch that decides the default"
  let answered ← Door.typedWith door (withProfile none) ["--json", "id"] none
  return settled do
    demand (!answered.succeeded) "a site was placed with nothing to place it by"
    let complaint ←
      match decodeComplaint answered.err with
      | .error reason => .error s!"the refusal did not parse: {reason}"
      | .ok complaint => .ok complaint
    demand (complaint.code == ⟨"kusanagi.no_root"⟩) s!"the refusal was {complaint.code}"
    demand (mentions complaint.recover "--root") "the way out does not name --root"

end Kusanagi.Doorway
