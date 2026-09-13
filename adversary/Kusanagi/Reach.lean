/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Check
import Kusanagi.Door
import Kusanagi.Ground
import Kusanagi.Listener
import Kusanagi.Stage

/-!
# What the client does when the network is not what it was told

Every property here is about failing closed. A host that never answers must
cost a bounded amount of time and not a hung agent; a host that answers
nonsense must produce a coded refusal and never a crash; a host that says "go
there instead" must not be obeyed, because "there" is where the client's
address would be learned; and a proxy, once named, must be the only thing the
client ever connects to — including when the proxy is down, which is the moment
a client that fell back to a direct connection would hand its address to the
host it was hiding from.
-/

namespace Kusanagi.Reach

open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Listener

/-- The first finding among several checks, and `held` when there is none. -/
private def firstFinding (findings : List Verdict) : Verdict :=
  (findings.find? Verdict.isBroken).getD .held

/-- The head of a stream, as far as a failure needs to quote it. -/
private def shown (bytes : ByteArray) : String :=
  (String.fromUTF8? (bytes.extract 0 (min bytes.size 200))).getD "(not UTF-8)"

/-- The same bytes with every ASCII capital lowered, so a search ignores case. -/
private def lowered (bytes : ByteArray) : ByteArray :=
  ⟨bytes.data.map fun byte => if byte ≥ 65 && byte ≤ 90 then byte + 32 else byte⟩

/-- The one request the door makes first: an invitation writes an offer. -/
private def inviting (door : Door) (site : System.FilePath) (surroundings : Surroundings)
    (locator : String) : IO Typed :=
  Door.typedWith door surroundings
    ["--root", site.toString, "--json", "invite", "--name", "-", "--waypoint", locator,
     "--for", "3600", "--can", "send,read"]
    (some "reaching-out\n".toUTF8)

/--
This process's environment with the proxy named, so the child is told
everything else it needs to start.
-/
private def withProxy (proxy : String) : Surroundings :=
  { changes := #[("KUSANAGI_PROXY", some proxy)] }

/-- This process's environment with the proxy gone: a new shell, a scheduler task. -/
private def withoutProxy : Surroundings := { changes := #[("KUSANAGI_PROXY", none)] }

private def notFound : ByteArray :=
  "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toUTF8

private def badGateway : ByteArray :=
  "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".toUTF8

/-- A refusal that carries a code, on the stream a refusal is written to. -/
private def refusedWithACode (what : String) (answer : Typed) : Verdict :=
  if answer.status == 0 then
    .broke s!"{what} was accepted: {shown answer.out}"
  else if answer.status == 1 then
    match decodeComplaint answer.err with
    | .ok complaint =>
      if complaint.code.stable != "" then .held
      else .broke s!"{what} was refused without a code"
    | .error reason =>
      .broke s!"{what} was refused in a shape this adversary cannot read: {reason}"
  else
    .broke s!"{what} made the process exit with {answer.status}: {shown answer.err}"

/-- Either door: an answer this adversary can read, or a refusal with a code. -/
private def oneOfTwoShapes (what : String) (answer : Typed) : Verdict :=
  if answer.status == 0 then
    match decodeOutcome answer.out with
    | .ok _ => .held
    | .error reason => .broke s!"{what} was accepted unreadably: {reason}"
  else if answer.status == 1 then
    refusedWithACode what answer
  else
    .broke s!"{what} made the process exit with {answer.status}: {shown answer.err}"

/-- How long a host that never answers may hold the verb, in nanoseconds. -/
private def bound : Nat := 90 * 1000000000

/-- Accepting the connection and never answering costs a bounded time. -/
def aBlackHoleIsRefusedInBoundedTime (door : Door) (ground : Ground) : IO Verdict :=
  withListener .blackHole fun hole => do
    let started ← IO.monoNanosNow
    let answer ← inviting door (ground.siteOf .alice) inherited hole.locator
    let finished ← IO.monoNanosNow
    let arrived ← hole.connections
    let took := finished - started
    return firstFinding
      [ ensure (arrived ≥ 1) "the client never connected to the black hole"
      , refusedWithACode "a host that never answers" answer
      , ensure (took < bound)
          s!"a host that never answers held the verb for {took / 1000000000} seconds" ]

/-- Every answer a host can give that is not a box's. -/
private def scripts : List (String × ByteArray) :=
  [ ("a body far shorter than its declared length",
     "HTTP/1.1 200 OK\r\nContent-Length: 999999\r\n\r\nxx".toUTF8)
  , ("bytes that are not HTTP",
     ByteArray.mk #[255, 254, 0, 1] ++ " this is not a protocol ".toUTF8 ++ ByteArray.mk #[0, 0])
  , ("a status line and nothing else", "HTTP/1.1 200 OK\r\n\r\n".toUTF8)
  , ("a 200 with a tiny body where a drop should be",
     "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".toUTF8)
  , ("a chunked body that never ends",
     "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n".toUTF8)
  , ("a header that never ends", String.ofList (List.replicate 20000 'x') |>.toUTF8)
  , ("a 500", "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".toUTF8) ]

/--
Answers that are not a box's are refused, and the process still leaves by one
of the two doors.
-/
def garbageIsRefusedNotCrashed (door : Door) (ground : Ground) : IO Verdict := do
  let findings ← scripts.mapM fun (what, bytes) =>
    withListener (.answer bytes) fun liar => do
      let answer ← inviting door (ground.siteOf .alice) inherited liar.locator
      return oneOfTwoShapes what answer
  return firstFinding findings

/--
A host that answers "go there instead" is refused, and "there" never hears from
the client.
-/
def aRedirectIsNeverFollowed (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun there =>
    withListener (.redirect there.locator) fun here => do
      let answer ← inviting door (ground.siteOf .alice) inherited here.locator
      let followed ← there.connections
      return firstFinding
        [ refusedWithACode "a redirecting host" answer
        , ensure (followed == 0) s!"the client followed a redirect {followed} time(s)" ]

/-- With a proxy named, the host is never connected to directly. -/
def theHostIsNeverReachedDirectlyWithAProxy (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun host =>
    withListener (.answer badGateway) fun proxy => do
      let surroundings := withProxy s!"http://127.0.0.1:{proxy.port}"
      let answer ← inviting door (ground.siteOf .alice) surroundings host.locator
      let direct ← host.connections
      let viaProxy ← proxy.connections
      return firstFinding
        [ ensure (viaProxy ≥ 1) "the proxy was named and never connected to"
        , ensure (direct == 0)
            s!"the host was reached directly {direct} time(s) with a proxy named"
        , refusedWithACode "a proxy that refuses to connect" answer ]

/--
A site that recorded "never without a proxy" refuses when the variable is gone
— a new shell, a scheduler task — rather than going direct.
-/
def aRequiredProxyThatIsMissingFailsClosed (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun host => do
    let alice := ground.siteOf .alice
    let recorded ←
      Door.typedWith door withoutProxy ["--root", alice.toString, "--json", "proxy", "--require"] none
    let answer ← inviting door alice withoutProxy host.locator
    let direct ← host.connections
    let coded :=
      match decodeComplaint answer.err with
      | .ok complaint =>
        if complaint.code.stable == "kusanagi.proxy_required" then .held
        else .broke s!"refused with `{complaint.code}` rather than `kusanagi.proxy_required`"
      | .error reason => .broke s!"refused in a shape this adversary cannot read: {reason}"
    return firstFinding
      [ ensure recorded.succeeded s!"recording the requirement failed: {recorded.status}"
      , ensure (direct == 0)
          s!"the host was reached directly {direct} time(s) with a proxy required and none set"
      , coded ]

/-- A proxy that is down is a refusal, not a direct connection. -/
def aDeadProxyFailsClosed (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun host => do
    let dying := ["socks5://127.0.0.1:1", "http://127.0.0.1:1", "socks5h://127.0.0.1:1"]
    let findings ← dying.mapM fun dead => do
      let answer ← inviting door (ground.siteOf .alice) (withProxy dead) host.locator
      let direct ← host.connections
      return firstFinding
        [ ensure (direct == 0) s!"with {dead} down, the host was reached directly"
        , refusedWithACode s!"a dead proxy at {dead}" answer ]
    return firstFinding findings

/-- The headers ordinary traffic carries, and nothing else. -/
private def ordinary : List String :=
  ["accept", "cache-control", "content-length", "content-type", "host", "if-none-match"]

/--
The lowercased header names of one request.

Only the head before the blank line: the body is 131 072 sealed bytes and any
of them can look like a header line.
-/
private def headerNames (request : ByteArray) : List String :=
  let text := (String.fromUTF8? request).getD ""
  let head := ((text.splitOn "\r\n\r\n").head?).getD ""
  ((head.splitOn "\r\n").drop 1).filterMap fun line =>
    match line.splitOn ":" with
    | name :: _ :: _ => some (String.map Char.toLower name)
    | _ => none

/-- The distinct members of a list of names, in order. -/
private def distinct (names : List String) : List String :=
  (names.foldl (fun kept name => if kept.contains name then kept else kept ++ [name]) []).mergeSort
    fun left right => !decide (right < left)

/--
Every request head carries only the headers ordinary traffic carries, no user
agent, and nothing that names this project.
-/
def theRequestHeadNamesNothing (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun host => do
    let _ ← inviting door (ground.siteOf .alice) inherited host.locator
    let seen ← host.heads
    let strange := distinct ((seen.flatMap headerNames).filter (!ordinary.contains ·))
    let telling := seen.filter fun request => Stage.contains "kusanagi".toUTF8 (lowered request)
    return firstFinding
      [ ensure (!seen.isEmpty) "no request head arrived"
      , ensure strange.isEmpty
          s!"a request carried a header ordinary traffic does not: {strange}"
      , match telling.head? with
        | none => .held
        | some request => .broke s!"a request names the project: {shown request}" ]

/--
The parts of the platform triple long enough to be a name rather than a
fragment.

`System.Platform.target` is where this toolchain says what machine it is
running on, so the operating system and the architecture arrive together in one
string and are split apart here.
-/
private def platformNames : List String :=
  (System.Platform.target.splitOn "-").filter (·.length ≥ 4)

/--
Nothing that names this machine, this account, this build or this moment is in
any byte that leaves it: not in a request head, not in an object the host
stores, not in the invitation line a person carries.
-/
def nothingThatNamesThisMachineLeavesIt (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer notFound) fun host => do
    let alice := ground.siteOf .alice
    let _ ← inviting door alice inherited host.locator
    let minted ← Door.ask door alice (.invite ⟨"carried-by-hand"⟩ ground.waypoint .forever both)
    let carried ←
      match minted with
      | .accepted (.invited _ line _) => pure line
      | other => throw <| IO.userError s!"the invitation was refused: {repr other}"
    let _ ← Door.ask door (ground.siteOf .bob) (.join carried ⟨"carried-by-hand"⟩)
    let seen ← host.heads
    let held ← ground.stored
    let version ← Door.typed door ["--version"] none
    let machine ← ["COMPUTERNAME", "USERNAME", "USERPROFILE", "HOSTNAME", "USER", "HOME",
      "LOGNAME"].mapM fun name => (IO.getEnv name : IO (Option String))
    let processor ← IO.getEnv "PROCESSOR_IDENTIFIER"
    let named :=
      (machine.filterMap id).filter (·.length ≥ 4)
        ++ [alice.toString] ++ platformNames ++ ["windows", "rustc", "cargo"]
        ++ [((String.fromUTF8? version.out).getD "").trimAscii.toString] ++ processor.toList
    let needles := (named.filter (!·.isEmpty)).map fun needle => (needle, lowered needle.toUTF8)
    let outgoing :=
      seen.map (fun request => ("a request head", request))
        ++ held.map (fun (_, bytes) => ("a host object", bytes))
        ++ [("the invitation line", carried.line.toUTF8)]
    let hit := needles.findSome? fun (rendered, needle) =>
      (outgoing.find? fun (_, bytes) => Stage.contains needle (lowered bytes)).map
        fun (place, _) => (rendered, place)
    match hit with
    | none => return .held
    | some (rendered, place) => return .broke s!"{place} carries {rendered}"

/--
Verbs that name no host make no connection, and the one that does makes the
number its protocol needs and not one more. The proxy is the oracle: named, it
is the only place a connection can go.
-/
def noVerbConnectsMoreThanItMust (door : Door) (ground : Ground) : IO Verdict :=
  withListener (.answer badGateway) fun proxy => do
    let surroundings := withProxy s!"http://127.0.0.1:{proxy.port}"
    let alice := ground.siteOf .alice
    let quietly (arguments : List String) : IO Typed :=
      Door.typedWith door surroundings (["--root", alice.toString, "--json"] ++ arguments) none
    let _ ← quietly ["id"]
    let _ ← quietly ["channels"]
    let _ ← quietly ["export"]
    let _ ← quietly ["--version"]
    let silent ← proxy.connections
    let _ ← inviting door alice surroundings "http://127.0.0.1:9/"
    let speaking ← proxy.connections
    return firstFinding
      [ ensure (silent == 0) s!"verbs that name no host made {silent} connection(s)"
      , ensure (speaking - silent == 1)
          s!"one refused request made {speaking - silent} connection(s)" ]

/-- Lowercase hexadecimal for one nibble. -/
private def digit (value : Nat) : Char := "0123456789abcdef".toList.getD value '0'

/-- Every character of a locator as the two hexadecimal digits of its code. -/
private def hexOfString (text : String) : String :=
  String.ofList (text.toList.flatMap fun letter =>
    [digit (letter.toNat / 16), digit (letter.toNat % 16)])

/-- The refusal a locator earned, provided it is not the one a dead host earns. -/
private def codeUnlike (dead : Code) (refused accepted : String) : Answer → Except String Code
  | .refused complaint =>
    if complaint.code.stable != dead.stable then .ok complaint.code
    else .error s!"{refused} {complaint.code}, the code a dead host gets: it was connected to"
  | .accepted outcome => .error s!"{accepted}: {repr outcome}"

/-- The UNC and UNC-shaped locators a network path can be written as. -/
private def networkPaths : List String :=
  ["\\\\127.0.0.1\\nothing\\drops", "//127.0.0.1/nothing/drops", "\\\\?\\UNC\\127.0.0.1\\nothing"]

/--
A locator that names a network path is refused at both ends of an invitation
with one code, and that code is not a network failure's: the refusal is a
decision about the string, made before anything is connected to.

A UNC path is a network connection the operating system makes on this program's
behalf, outside any proxy it was told to use, to a machine the inviter chose —
and on Windows that connection authenticates. A dead drop on a file share is
still possible: the person mounts it, and the program sees a drive letter.
-/
def aLocatorNeverNamesANetworkPath (door : Door) (ground : Ground) : IO Verdict := do
  let alice := ground.siteOf .alice
  let minting (locator : String) : IO Answer :=
    Door.ask door alice
      (.invite ⟨"via-" ++ String.ofList (locator.toList.filter fun c => 'a' ≤ c && c ≤ 'z')⟩
        locator .forever both)
  let stranger ← minting "http://127.0.0.1:1/"
  let unc ← networkPaths.mapM minting
  let genuine ← Door.ask door alice (.invite ⟨"genuine"⟩ ground.waypoint .forever both)
  let joined ←
    match genuine with
    | .accepted (.invited _ line _) =>
      let secret := (((line.line.dropWhile (· != ':')).drop 1).take 132)
      let forged : Invite :=
        ⟨"kusanagi2:" ++ secret ++ hexOfString "\\\\127.0.0.1\\nothing\\drops"⟩
      some <$> Door.ask door (ground.siteOf .bob) (.join forged ⟨"forged"⟩)
    | _ => pure none
  match stranger with
  | .accepted outcome => return .broke s!"a dead host was accepted: {repr outcome}"
  | .refused complaint =>
    let dead := complaint.code
    let mut codes : List Code := []
    for answer in unc do
      match codeUnlike dead "a UNC locator was refused with"
          "a UNC locator was accepted at invite" answer with
      | .error why => return .broke why
      | .ok code => codes := codes ++ [code]
    match joined with
    | none => return .broke s!"no genuine invitation to forge from: {repr genuine}"
    | some answer =>
      match codeUnlike dead "an invitation carrying a UNC locator was refused at join with"
          "an invitation carrying a UNC locator was accepted at join" answer with
      | .error why => return .broke why
      | .ok atJoin =>
        let other := codes.filter (·.stable != atJoin.stable)
        return ensure other.isEmpty
          s!"a network path is refused with {other.map Code.stable} at invite and {atJoin} at join"

end Kusanagi.Reach
