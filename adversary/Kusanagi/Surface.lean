/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Sync.Mutex
import Kusanagi.Check
import Kusanagi.Custody
import Kusanagi.Door
import Kusanagi.Forging
import Kusanagi.Glass
import Kusanagi.Ground
import Kusanagi.Insider
import Kusanagi.Journey
import Kusanagi.Leakage
import Kusanagi.Port
import Kusanagi.Reach
import Kusanagi.Room
import Kusanagi.Scanner
import Kusanagi.Sweep
import Kusanagi.Terminal
import Kusanagi.Twins

/-!
# The attack-surface matrix of `surface-SPEC.md`, one claim per cell

Grouped by where the adversary stands, so that a red line says who it is that
learned something. Every claim is a relation in one throwaway world; the names
are the sentences a reviewer is meant to be able to disagree with.
-/

namespace Kusanagi.Surface

open Kusanagi.Check
open Kusanagi.Door (Door)
open Kusanagi.Ground

/-- A reason is a broken claim; no reason is a claim that held. -/
private def settled : Except String Unit → Verdict
  | .ok _ => .held
  | .error reason => .broke reason

/--
One cell of the matrix: one throwaway world, one relation, one sentence to
disagree with.

The relations that answer with a reason are wrapped here; the ones that already
answer with a verdict go through `judged` instead. The two shapes are what the
ported modules hand back, not a distinction the matrix draws.
-/
private def cell (door : Door) (name : String)
    (act : Door → Ground → IO (Except String Unit)) : Suite :=
  .claim name (withGround fun ground => return settled (← act door ground))

/-- The same cell, for a relation that already answers with a verdict. -/
private def judged (door : Door) (name : String)
    (act : Door → Ground → IO Verdict) : Suite :=
  .claim name (withGround (act door))

/-- The matrix, grouped by where the adversary stands. -/
def surface (door : Door) : Suite :=
  .group "the attack surface"
    [ .group "the host, holding every object"
        [ cell door "holds no word anybody said, no name and no secret"
            Leakage.hostHoldsNoWord
        , cell door
            "holds no self-chosen name in the clear, though both ends see the other's"
            Leakage.namesRideSealed
        , cell door "cannot pair one identity's two channels"
            Leakage.twoChannelsShareNothing
        , cell door "cannot pair one identity across two hosts"
            Leakage.twoHostsShareNothing
        , cell door "cannot pair the two drops of one broadcast"
            Insider.aBroadcastIsTwoStrangers
        , judged door "is refused when it changes the first, a middle or the last byte"
            Forging.anyByteFlippedIsRefused
        , judged door
            "is refused when it serves the wrong shape, and cannot squat the next address"
            Forging.aWrongShapeIsNotASegment
        , judged door "cannot pass the peer's drop off as the author's"
            Forging.aPeersDropIsNotTheAuthors
        , judged door "cannot pass another channel's drop off as this one's"
            Forging.anotherChannelsDropIsRefused
        , judged door "cannot make a gap be skipped, nor a reader forget across it"
            Forging.aGapStopsAFreshReaderAndMovesNobodyBack
        , judged door "cannot swap two drops" Forging.swappedDropsAreRefused
        , judged door "changes nothing by adding a hundred objects"
            Forging.junkChangesNothing
        , judged door
            "cannot reset a spent invitation once the inviter has seen who accepted"
            Forging.theFirstPeerIsPinned
        , judged door "loses a released drop, and cannot bring it back"
            Forging.aReleasedDropIsGoneAndStaysGone
        , judged door "restored from a backup, stops the author as well as the reader"
            Forging.aRolledBackHostStopsTheAuthorToo
        , cell door
            "sees a read fetch the whole bin, strangers included, and report only its own"
            Sweep.aReadFetchesTheWholeBinAndNothingElse
        , cell door "sees no request name an address outside a bin the same side listed"
            Sweep.noRequestNamesAnUnlistedAddress ]
    , .group "the network, misbehaving"
        [ judged door "a host that never answers costs bounded time"
            Reach.aBlackHoleIsRefusedInBoundedTime
        , judged door "a host that answers garbage is refused, never crashed on"
            Reach.garbageIsRefusedNotCrashed
        , judged door "a redirect is never followed" Reach.aRedirectIsNeverFollowed
        , judged door "with a proxy named, the host is never reached directly"
            Reach.theHostIsNeverReachedDirectlyWithAProxy
        , judged door "a dead proxy fails closed" Reach.aDeadProxyFailsClosed
        , judged door "a required proxy that is missing fails closed"
            Reach.aRequiredProxyThatIsMissingFailsClosed
        , judged door "the request head names nothing" Reach.theRequestHeadNamesNothing
        , judged door "a locator never names a network path"
            Reach.aLocatorNeverNamesANetworkPath
        , judged door "nothing that names this machine leaves it"
            Reach.nothingThatNamesThisMachineLeavesIt
        , judged door "no verb connects more than it must"
            Reach.noVerbConnectsMoreThanItMust ]
    , .group "a second account, or whoever has the disk"
        [ cell door "finds no message in a site" Leakage.theSiteHoldsNoMessage
        , cell door "finds no name and no secret in a site's bytes"
            Leakage.theSiteHoldsNoName
        , cell door "finds no file named after anybody" Leakage.noFileIsNamedAfterAnybody
        , cell door "finds no two files with one name in a site"
            Leakage.noTwoFilesShareAName
        , cell door "cannot join two seized sites on a file name"
            Leakage.twoSitesShareNoFilename
        , cell door "finds nothing in an archive without its key" Leakage.theArchiveIsOpaque
        , cell door "hears the secret half of an invitation exactly once"
            Leakage.theSecretIsSaidOnce
        , cell door
            "cannot import with the wrong key, a damaged archive or into an occupied root"
            Custody.importRefusesWhatIsNotItsKey
        , cell door "restores a site that reads exactly the same"
            Custody.aRestoredSiteReadsTheSame
        , cell door "forgets a channel without touching the host"
            Custody.forgettingLeavesNothingBehind
        , cell door "leaves nothing half-written" Custody.nothingIsLeftHalfWritten
        , cell door "changes nothing on disk when a verb is refused"
            Custody.aRefusedVerbChangesNothingOnDisk
        , cell door "cannot change who you are by damaging any one file"
            Custody.aCorruptedFileNeverChangesWhoYouAre ]
    , .group "the peer, at the reader's terminal"
        [ judged door "cannot change the program's words, only their length"
            Terminal.theProgramsWordsDependOnlyOnLength
        , judged door "cannot put a control byte on the terminal"
            Terminal.noControlByteReachesTheTerminal
        , judged door "cannot have a large payload silently cut"
            Terminal.bigPayloadsAreWholeOrRefused ]
    , .group "the peer, at the agent's port"
        [ judged door "is fenced in a tool result exactly as at the terminal"
            Port.theToolResultIsFencedLikeTheTerminal
        , judged door "finds the tools are the verbs" Port.theToolsAreTheVerbs ]
    , .group "a member of a group, and a peer who was cut off"
        [ cell door "learns no other member" Insider.aMemberLearnsNoOtherMember
        , cell door "cannot tell a broadcast from a whisper"
            Insider.aBroadcastLooksLikeAWhisper
        , cell door "is left out of a fan-out once revoked" Insider.aRevokedMemberIsLeftOut
        , cell door "cannot be sent to once revoked, for the same reason as read"
            Insider.sendingToTheRevokedFailsLikeReadingThem ]
    , .group "a member of a room, and the host that holds it"
        [ judged door "can name every other member: the price of a room"
            Room.aMemberCanListEveryMember
        , judged door "leaves the host no handle, no room name and no sentence"
            Room.theHostHoldsNoMember ]
    , .group "two writers who are one author"
        [ cell door "a restored twin cannot fork the stream" Twins.aRestoredTwinCannotFork
        , cell door "eight sends at once cannot fork the stream"
            Twins.parallelSendsNeverFork ]
    , .group "a scanner, holding no address"
        [ cell door "gets one answer whatever it asks"
            Scanner.everyStrangerGetsTheSameAnswer
        , cell door "cannot store an object of the wrong size"
            Scanner.aWrongSizeIsNeverStored
        , cell door "cannot overwrite an address" Scanner.anAddressIsWrittenOnce
        , cell door "cannot climb out of the directory" Scanner.traversalTouchesNothing ] ]

/--
One window at a time: the automation server and the process are singletons, so
the cells below hold one lock and run one after another whatever the runner's
`abreast` says.
-/
private def alone (lock : Std.Mutex Unit) (door : Door) (name : String)
    (act : Door → Ground → IO (Except String Unit)) : Suite :=
  .claim name <|
    lock.atomically <| liftM <| withGround fun ground => return settled (← act door ground)

/--
The window against a rogue peer (H8), when the window has been built.

CI never builds the GUI, so this group answers "skipped" rather than red where
there is nothing to drive; on the machine that ships it is a gate.
-/
def window (door : Door) : IO Suite := do
  match ← Glass.available with
  | none =>
    return .claim "the window, rendering a peer" <| pure <| .skipped
      "the window is not built (`native build -Dautomation=true -Dtrace=off` in glass/)"
  | some _ =>
    let lock ← Std.Mutex.new ()
    return .group "the window, rendering a peer"
      [ alone lock door "never fetches an image the peer named"
          Glass.aRemoteImageIsNeverFetched
      , alone lock door "binds nothing to a link, javascript: and file: included"
          Glass.aLinkCannotBePressed
      , alone lock door "shows terminal bytes as hexadecimal"
          Glass.controlBytesAreShownAsHex
      , alone lock door "writes nothing of the peer outside the site"
          Glass.theDiskHoldsNoPeer
      , alone lock door "writes the clipboard only by hand, and says what the clipboard is"
          Glass.theClipboardWaitsForAHand
      , alone lock door "starts a conversation from the sheet, and hears the reply"
          Journey.aConversationStartsInTheWindow
      , alone lock door "founds a room from the sheet, and hears a member"
          Journey.aRoomIsFoundedInTheWindow ]

end Kusanagi.Surface
