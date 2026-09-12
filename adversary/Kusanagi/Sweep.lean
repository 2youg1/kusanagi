/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Ground
import Kusanagi.Relay
import Kusanagi.Stage

/-!
# What a host's access log says about a read, now that a read names a bin

The relay in front of a real host sees every request line. Before D-20 a
reader asked `GET /address` and the log paired the writer of that address with
its reader. Now a reader asks `GET /bin/period/ward/` and then fetches every
key the answer listed — strangers' objects included — so the log holds a ward
being read and never which object in it was wanted.

Two properties, both relations between the log and the disk rather than an
expected trace: **a read fetches exactly the bin**, and **no request of a read
or a send names an address outside a bin the same command listed**.
-/

namespace Kusanagi.Sweep

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground
open Kusanagi.Relay
open Kusanagi.Stage

/-- The request paths the relay saw after a point, with the method that made them. -/
private def since (skipped : Nat) (seen : List Observation) : List (String × String) :=
  (seen.drop skipped).map fun o => (o.observedMethod, o.observedPath)

/-- Every key on the host that sits in `bin`. -/
private def inBin (ground : Ground) (bin : String) : IO (List String) := do
  let held ← ground.stored
  let keys := held.filterMap fun (address, _) =>
    if binOf address == bin then some address.key else none
  return keys.mergeSort fun left right => left ≤ right

/-- The first failure among the findings, or nothing to report. -/
private def allOf (findings : List (Except String Unit)) : Except String Unit :=
  findings.forM fun finding => finding

/-- The paths of the `GET`s that fetched an object, without their `/d/`. -/
private def fetchedBy (requests : List (String × String)) : List String :=
  requests.filterMap fun (method, path) =>
    if method == "GET" && "/d/".isPrefixOf path then some (path.drop 3).toString else none

/-- The paths of the `GET`s that listed a bin. -/
private def listingsIn (requests : List (String × String)) : List String :=
  requests.filterMap fun (method, path) =>
    if method == "GET" && "/bin/".isPrefixOf path then some path else none

/-- Forty of one letter, which is the shape of a key a host files an object under. -/
private def fortyOf (letter : Char) : String := String.ofList (List.replicate 40 letter)

/-- A listing prefix with the trailing separators a host writes taken off again. -/
private def withoutTrailingSlashes (asked : String) : String :=
  String.ofList (asked.toList.reverse.dropWhile (· == '/')).reverse

/--
Alice writes three drops to Bob; the host adds two objects of its own to Bob's
bin. Bob's first read fetches every object in the bin — his three and the two
strangers — and reports exactly his three. A reader that fetched a subset would
be telling the host which subset was its own.
-/
def aReadFetchesTheWholeBinAndNothingElse (door : Door) (ground : Ground) :
    IO (Except String Unit) :=
  withRelay door ground.waypoint fun relay => do
    let stage ← talkWith door ground .alice .bob
      (.invite (fresh "taken-whole") (System.FilePath.mk relay.locator) .forever both)
    let said ← ["one", "two", "three"].mapM (say door stage.writer stage.channel)
    let bin ←
      match said with
      | first :: _ => pure (binOf first)
      | [] => throw <| IO.userError "nothing was said"
    for letter in ['a', 'b'] do
      let filler : UInt8 := if letter == 'a' then 0 else 1
      ground.plant ⟨bin ++ "/" ++ fortyOf letter⟩
        (ByteArray.mk (Array.replicate 131072 filler))
    let before := (← relay.observed).length
    let heard ← hear door stage.reader stage.channel
    let requests := since before (← relay.observed)
    let everything ← inBin ground bin
    let fetched := (fetchedBy requests).mergeSort fun left right => left ≤ right
    let listings := listingsIn requests
    let counted : Except String Unit :=
      match entriesOf heard with
      | .ok three =>
        if three.length == 3 then .ok ()
        else .error s!"a bin with two strangers in it changed the read: {repr three}"
      | .error why => .error s!"a bin with two strangers in it changed the read: {why}"
    return allOf [
      if listings.isEmpty then .error "the read listed no bin" else .ok (),
      if fetched == everything then .ok ()
        else .error s!"the read fetched {fetched} and the bin holds {everything}",
      counted]

/--
Across an invitation, three sends and two reads, every request that names an
object names one under a bin the same side listed first, and every listing
names a period and a ward and nothing more. The rendezvous — the offer and the
greeting, in period zero — is the one exception, fetched by address once and
written down as such.
-/
def noRequestNamesAnUnlistedAddress (door : Door) (ground : Ground) :
    IO (Except String Unit) :=
  withRelay door ground.waypoint fun relay => do
    let stage ← talkWith door ground .alice .bob
      (.invite (fresh "never-an-address") (System.FilePath.mk relay.locator) .forever both)
    let afterJoin := (← relay.observed).length
    let _ ← ["one", "two", "three"].mapM (say door stage.writer stage.channel)
    let _ ← hear door stage.reader stage.channel
    let _ ← say door stage.reader stage.channel "four"
    let _ ← hear door stage.writer stage.channel
    let requests := since afterJoin (← relay.observed)
    let listed := (listingsIn requests).map fun path => (path.drop 5).toString
    -- Period zero is the rendezvous: the offer and the greeting sit there,
    -- fetched by address once, and `seal::rendezvous` says why.
    let named := requests.filterMap fun (_, path) =>
      if "/d/".isPrefixOf path && !"/d/0000000000000000/".isPrefixOf path then
        some (path.drop 3).toString
      else
        none
    let unlisted := named.filter fun key => !listed.any (·.isPrefixOf key)
    let malformed := listed.filter fun asked =>
      ((withoutTrailingSlashes asked).splitOn "/").length != 2
    return allOf [
      if listed.isEmpty then .error "nothing was listed" else .ok (),
      match unlisted with
      | [] => .ok ()
      | path :: _ =>
        .error s!"a request named {path}, which no listing on this side preceded",
      match malformed with
      | [] => .ok ()
      | asked :: _ =>
        .error s!"a listing asked for {asked}, which is not a period and a ward",
      if requests.any (·.1 == "DELETE") then .error "a delete named an address" else .ok ()]

end Kusanagi.Sweep
