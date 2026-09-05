-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

-- | The attack-surface matrix of `surface-SPEC.md`, one test per cell.
--
-- Grouped by where the adversary stands, so that a red line says who it is
-- that learned something. Every test is a relation in one throwaway world; the
-- names are the sentences a reviewer is meant to be able to disagree with.
{-# LANGUAGE LambdaCase #-}

module Surface (surface, window) where

import Test.Tasty (DependencyType (AllFinish), TestTree, dependentTestGroup, testGroup)
import Test.Tasty.HUnit (assertBool, testCase)

import Kusanagi.Custody qualified as Custody
import Kusanagi.Door (Door)
import Kusanagi.Forging qualified as Forging
import Kusanagi.Glass qualified as Glass
import Kusanagi.Ground (withGround)
import Kusanagi.Insider qualified as Insider
import Kusanagi.Leakage qualified as Leakage
import Kusanagi.Port qualified as Port
import Kusanagi.Reach qualified as Reach
import Kusanagi.Scanner qualified as Scanner
import Kusanagi.Terminal qualified as Terminal
import Kusanagi.Twins qualified as Twins

surface :: Door -> TestTree
surface door =
  testGroup
    "the attack surface"
    [ testGroup
        "the host, holding every object"
        [ cell "holds no word anybody said, no name and no secret" Leakage.hostHoldsNoWord
        , cell "cannot pair one identity's two channels" Leakage.twoChannelsShareNothing
        , cell "cannot pair one identity across two hosts" Leakage.twoHostsShareNothing
        , cell "cannot pair the two drops of one broadcast" Insider.aBroadcastIsTwoStrangers
        , cell "is refused when it changes the first, a middle or the last byte" Forging.anyByteFlippedIsRefused
        , cell "is refused when it serves the wrong shape, and cannot squat the next address" Forging.aWrongShapeIsNotASegment
        , cell "cannot pass the peer's drop off as the author's" Forging.aPeersDropIsNotTheAuthors
        , cell "cannot pass another channel's drop off as this one's" Forging.anotherChannelsDropIsRefused
        , cell "cannot make a gap be skipped, nor a reader forget across it" Forging.aGapStopsAFreshReaderAndMovesNobodyBack
        , cell "cannot swap two drops" Forging.swappedDropsAreRefused
        , cell "changes nothing by adding a hundred objects" Forging.junkChangesNothing
        , cell "cannot reset a spent invitation once the inviter has seen who accepted" Forging.theFirstPeerIsPinned
        , cell "loses a released drop, and cannot bring it back" Forging.aReleasedDropIsGoneAndStaysGone
        , cell "restored from a backup, stops the author as well as the reader" Forging.aRolledBackHostStopsTheAuthorToo
        ]
    , testGroup
        "the network, misbehaving"
        [ cell "a host that never answers costs bounded time" Reach.aBlackHoleIsRefusedInBoundedTime
        , cell "a host that answers garbage is refused, never crashed on" Reach.garbageIsRefusedNotCrashed
        , cell "a redirect is never followed" Reach.aRedirectIsNeverFollowed
        , cell "with a proxy named, the host is never reached directly" Reach.theHostIsNeverReachedDirectlyWithAProxy
        , cell "a dead proxy fails closed" Reach.aDeadProxyFailsClosed
        , cell "a required proxy that is missing fails closed" Reach.aRequiredProxyThatIsMissingFailsClosed
        , cell "the request head names nothing" Reach.theRequestHeadNamesNothing
        , cell "a locator never names a network path" Reach.aLocatorNeverNamesANetworkPath
        , cell "nothing that names this machine leaves it" Reach.nothingThatNamesThisMachineLeavesIt
        , cell "no verb connects more than it must" Reach.noVerbConnectsMoreThanItMust
        ]
    , testGroup
        "a second account, or whoever has the disk"
        [ cell "finds no message in a site" Leakage.theSiteHoldsNoMessage
        , cell "finds no name and no secret in a site's bytes" Leakage.theSiteHoldsNoName
        , cell "finds no file named after anybody" Leakage.noFileIsNamedAfterAnybody
        , cell "finds no two files with one name in a site" Leakage.noTwoFilesShareAName
        , cell "cannot join two seized sites on a file name" Leakage.twoSitesShareNoFilename
        , cell "finds nothing in an archive without its key" Leakage.theArchiveIsOpaque
        , cell "hears the secret half of an invitation exactly once" Leakage.theSecretIsSaidOnce
        , cell "cannot import with the wrong key, a damaged archive or into an occupied root" Custody.importRefusesWhatIsNotItsKey
        , cell "restores a site that reads exactly the same" Custody.aRestoredSiteReadsTheSame
        , cell "forgets a channel without touching the host" Custody.forgettingLeavesNothingBehind
        , cell "leaves nothing half-written" Custody.nothingIsLeftHalfWritten
        , cell "changes nothing on disk when a verb is refused" Custody.aRefusedVerbChangesNothingOnDisk
        , cell "cannot change who you are by damaging any one file" Custody.aCorruptedFileNeverChangesWhoYouAre
        ]
    , testGroup
        "the peer, at the reader's terminal"
        [ cell "cannot change the program's words, only their length" Terminal.theProgramsWordsDependOnlyOnLength
        , cell "cannot put a control byte on the terminal" Terminal.noControlByteReachesTheTerminal
        , cell "cannot have a large payload silently cut" Terminal.bigPayloadsAreWholeOrRefused
        ]
    , testGroup
        "the peer, at the agent's port"
        [ cell "is fenced in a tool result exactly as at the terminal" Port.theToolResultIsFencedLikeTheTerminal
        , cell "finds the tools are the verbs" Port.theToolsAreTheVerbs
        ]
    , testGroup
        "a member of a group, and a peer who was cut off"
        [ cell "learns no other member" Insider.aMemberLearnsNoOtherMember
        , cell "cannot tell a broadcast from a whisper" Insider.aBroadcastLooksLikeAWhisper
        , cell "is left out of a fan-out once revoked" Insider.aRevokedMemberIsLeftOut
        , cell "cannot be sent to once revoked, for the same reason as read" Insider.sendingToTheRevokedFailsLikeReadingThem
        ]
    , testGroup
        "two writers who are one author"
        [ cell "a restored twin cannot fork the stream" Twins.aRestoredTwinCannotFork
        , cell "eight sends at once cannot fork the stream" Twins.parallelSendsNeverFork
        ]
    , testGroup
        "a scanner, holding no address"
        [ cell "gets one answer whatever it asks" Scanner.everyStrangerGetsTheSameAnswer
        , cell "cannot store an object of the wrong size" Scanner.aWrongSizeIsNeverStored
        , cell "cannot overwrite an address" Scanner.anAddressIsWrittenOnce
        , cell "cannot climb out of the directory" Scanner.traversalTouchesNothing
        ]
    ]
  where
    cell name act = testCase name (withGround (act door) >>= either (`assertBool` False) pure)

-- | The window against a rogue peer (H8), when the window has been built.
--
-- CI never builds the GUI, so this group answers "skipped" rather than red
-- where there is nothing to drive; on the machine that ships it is a gate.
window :: Door -> IO TestTree
window door =
  Glass.available >>= \case
    Nothing ->
      pure (testCase "skipped: the window is not built (`native build -Dautomation=true -Dtrace=off` in glass/)" (pure ()))
    Just _ ->
      -- One window at a time: the automation server and the process are
      -- singletons, so the cells run one after another whatever `-N` says.
      pure $
        dependentTestGroup
          "the window, rendering a peer"
          AllFinish
          [ cell "never fetches an image the peer named" Glass.aRemoteImageIsNeverFetched
          , cell "binds nothing to a link, javascript: and file: included" Glass.aLinkCannotBePressed
          , cell "shows terminal bytes as hexadecimal" Glass.controlBytesAreShownAsHex
          , cell "writes nothing of the peer outside the site" Glass.theDiskHoldsNoPeer
          , cell "writes the clipboard only by hand, and says what the clipboard is" Glass.theClipboardWaitsForAHand
          ]
  where
    cell name act = testCase name (withGround (act door) >>= either (`assertBool` False) pure)
