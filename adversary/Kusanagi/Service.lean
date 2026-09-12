/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Door

/-!
# The two verbs whose shape is not one question and one answer

`ask` is every other verb; these two are the exceptions, and exceptions belong
apart rather than in the middle of the rule. An export's result is a file rather
than a report, and a host's result is a socket rather than a value. Each reads
both streams for what each one is.
-/

namespace Kusanagi.Service

open Kusanagi.Answer
open Kusanagi.Door

/--
Seals a site into an archive, and says the key that opens it once.

The one verb whose result is a file rather than a report: the archive goes to
stdout as bytes, the recovery key to stderr as JSON, and nothing keeps a copy of
that key. So this cannot go through `ask`, which reads stdout as an outcome; it
reads both streams for what each one is.
-/
def exporting (door : Door) (site : System.FilePath) :
    IO (Except Complaint (String × ByteArray)) := do
  let said ← Door.typed door ["--root", site.toString, "--json", "export"] none
  if said.succeeded then
    match decodeOutcome said.err with
    | .ok (.exported key) => return .ok (key, said.out)
    | .ok other =>
      throw <| IO.userError s!"export reported something other than a key: {repr other}"
    | .error reason =>
      throw <| IO.userError
        s!"export said its key in a shape this adversary cannot read: {reason}"
  else
    match decodeComplaint said.err with
    | .ok complaint => return .error complaint
    | .error reason => throw <| IO.userError s!"export refused unreadably: {reason}"

/-- The last word of a line, which is where the host announces its address. -/
private def lastWord (line : String) : Option String :=
  (((line.split Char.isWhitespace).toList.map (·.toString)).filter (!·.isEmpty)).getLast?

/--
Runs a host for the duration of an action, and hands over the address it took.

The one verb that never returns needs a shape of its own: the result of
`kusanagi host` is not a value on stdout but a socket somebody else can connect
to, and that address is announced on stderr as the last word of the first line.
**The address is asked for rather than chosen** (`--bind 0`), because a test
that picks a port has already lost a race with every other test on the machine.

Reading that line is also the readiness signal: it is written once the listener
is up, so an action that begins by connecting will find something there.
-/
def hosting (door : Door) (directory : System.FilePath) (act : String → IO α) : IO α := do
  let child ← IO.Process.spawn {
    cmd := door.binary.toString
    args := #["host", "--dir", directory.toString, "--bind", "0"]
    stdin := .null
    stdout := .piped
    stderr := .piped }
  try
    let announced ← child.stderr.getLine
    match lastWord announced with
    | some address => act address
    | none => throw <| IO.userError s!"the host announced no address: {repr announced}"
  finally
    -- The host is the one verb that never returns, so the action ending is the
    -- only signal that it is no longer wanted; the wait afterwards keeps a
    -- stopped child from being left behind for somebody else to reap.
    try child.kill catch _ => pure ()
    let _ ← child.wait

end Kusanagi.Service
