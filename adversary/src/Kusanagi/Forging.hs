-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | Everything else a host can do to the bytes it holds.
--
-- "Kusanagi.Lying" covers a moved object and a vanished one. This module takes
-- the rest of the host's power over its own disk: any byte, any shape, any
-- position, any object copied from anywhere, extra objects, and the one-time
-- address reset that lets a second person accept a spent invitation. Every
-- property is a relation between a reader's answer before the lie and after
-- it, and the shape of every acceptable answer is the same: a coded refusal,
-- or a height no greater than what was honestly verifiable, and never a
-- segment its author did not put there.
module Kusanagi.Forging
  ( anyByteFlippedIsRefused
  , aWrongShapeIsNotASegment
  , aPeersDropIsNotTheAuthors
  , anotherChannelsDropIsRefused
  , aGapStopsAFreshReaderAndMovesNobodyBack
  , swappedDropsAreRefused
  , junkChangesNothing
  , theFirstPeerIsPinned
  , aReleasedDropIsGoneAndStaysGone
  , aRolledBackHostStopsTheAuthorToo
  ) where

import Control.Monad (forM, forM_)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Word (Word64)

import Kusanagi.Answer
  ( Address (..)
  , Answer (..)
  , Carried (..)
  , ChannelName
  , Entry (..)
  , Outcome (..)
  , Summary (..)
  )
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground
import Kusanagi.Stage

-- | The first byte, a middle byte and the last byte — the last being inside
-- the pad — each refuse the object when changed.
anyByteFlippedIsRefused :: Door -> Ground -> IO (Either String ())
anyByteFlippedIsRefused door ground = do
  stage <- talk door ground Alice Bob (fresh "every-byte-is-checked")
  address <- say door (talkWriter stage) (talkChannel stage) "intact"
  original <- holding ground address
  let size = ByteString.length original
  findings <- forM [("first", 0), ("middle", size `div` 2), ("last", size - 1)] $ \(which, offset) -> do
    damage ground offset address
    answer <- hear door (talkReader stage) (talkChannel stage)
    plant ground address original
    pure (neverShown ("the " <> which <> " byte changed") "intact" answer)
  pure (sequence_ findings)

-- | Truncated, extended, zeroed, emptied or replaced with noise of the right
-- size: none of them is ever reported as a segment, and a later honest
-- segment is not reported either, because the chain has a hole.
aWrongShapeIsNotASegment :: Door -> Ground -> IO (Either String ())
aWrongShapeIsNotASegment door ground = do
  stage <- talk door ground Alice Bob (fresh "shape-is-checked")
  first <- say door (talkWriter stage) (talkChannel stage) "well formed"
  original <- holding ground first
  let size = ByteString.length original
      shapes =
        [ ("one byte short", ByteString.take (size - 1) original)
        , ("one byte long", original <> "\0")
        , ("all zero", ByteString.replicate size 0)
        , ("empty", ByteString.empty)
        , ("noise of the right size", ByteString.pack (take size (cycle [7, 91, 200, 13, 42, 255, 0, 128])))
        ]
  findings <- forM shapes $ \(which, bytes) -> do
    plant ground first bytes
    answer <- hear door (talkReader stage) (talkChannel stage)
    plant ground first original
    pure (neverShown which "well formed" answer)
  -- A squatted next address: whatever the host puts there, the author's next
  -- send is a coded refusal or lands beyond it, and the reader never sees junk.
  _ <- hear door (talkReader stage) (talkChannel stage)
  vanish ground first
  plant ground first (ByteString.replicate size 1)
  sent <- Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "after the squat")
  after <- hear door (talkReader stage) (talkChannel stage)
  let squat = case sent of
        Accepted (Sent _ index _) | index > 0 -> Right ()
        Accepted other -> Left ("a send over a squatted address reported " <> show other)
        Refused _ -> Right ()
  pure (sequence_ findings >> squat >> neverShownBytes "a squatted address" (ByteString.replicate size 1) after)

-- | The peer's own genuine drop, served at the author's next address, is not
-- the author's segment: the key is derived from who wrote, not only where.
aPeersDropIsNotTheAuthors :: Door -> Ground -> IO (Either String ())
aPeersDropIsNotTheAuthors door ground = do
  stage <- talk door ground Alice Bob (fresh "authors-are-not-interchangeable")
  alice <- mapM (say door (talkWriter stage) (talkChannel stage)) ["alice one", "alice two"]
  bob <- say door (talkReader stage) (talkChannel stage) "bob speaking"
  case alice of
    [_, second] -> do
      vanish ground second
      transplant ground bob second
      answer <- hear door (talkReader stage) (talkChannel stage)
      pure (neverShown "the peer's drop at the author's address" "bob speaking" answer)
    _ -> pure (Left "the author did not write two segments")

-- | A genuine drop from another channel, served at this channel's address,
-- opens under neither key.
anotherChannelsDropIsRefused :: Door -> Ground -> IO (Either String ())
anotherChannelsDropIsRefused door ground = do
  here <- talk door ground Alice Bob (fresh "this-channel")
  there <- talk door ground Alice Mallory (fresh "that-channel")
  mine <- say door (talkWriter here) (talkChannel here) "for bob"
  theirs <- say door (talkWriter there) (talkChannel there) "for mallory"
  vanish ground mine
  transplant ground theirs mine
  answer <- hear door (talkReader here) (talkChannel here)
  pure (neverShown "another channel's drop" "for mallory" answer)

-- | A missing middle segment stops a reader with no memory below the gap, and
-- moves a reader with memory nowhere at all.
aGapStopsAFreshReaderAndMovesNobodyBack :: Door -> Ground -> IO (Either String ())
aGapStopsAFreshReaderAndMovesNobodyBack door ground = do
  stage <- talk door ground Alice Bob (fresh "no-gap-is-skipped")
  addresses <- mapM (say door (talkWriter stage) (talkChannel stage)) ["zero", "one", "two", "three"]
  case addresses of
    [_, second, _, _] -> do
      middle <- holding ground second
      vanish ground second
      fresh' <- hear door (talkReader stage) (talkChannel stage)
      plant ground second middle
      remembered <- hear door (talkReader stage) (talkChannel stage)
      vanish ground second
      again <- hear door (talkReader stage) (talkChannel stage)
      pure $ do
        boundedBelow "a reader with no memory" 1 fresh'
        atLeast "a reader that verified to three" 3 remembered
        atLeast "a reader that verified to three, after the gap reopened" 3 again
    _ -> pure (Left "the author did not write four segments")

-- | Two adjacent drops with their contents exchanged are refused, or the read
-- stops below the first of them.
swappedDropsAreRefused :: Door -> Ground -> IO (Either String ())
swappedDropsAreRefused door ground = do
  stage <- talk door ground Alice Bob (fresh "order-is-in-the-key")
  addresses <- mapM (say door (talkWriter stage) (talkChannel stage)) ["first", "second"]
  case addresses of
    [first, second] -> do
      a <- holding ground first
      b <- holding ground second
      plant ground first b
      plant ground second a
      answer <- hear door (talkReader stage) (talkChannel stage)
      pure (neverShown "swapped drops" "first" answer >> neverShown "swapped drops" "second" answer)
    _ -> pure (Left "the author did not write two segments")

-- | A hundred extra objects on the host change nothing a reader reports,
-- because a reader derives addresses and never lists what the host has.
junkChangesNothing :: Door -> Ground -> IO (Either String ())
junkChangesNothing door ground = do
  stage <- talk door ground Alice Bob (fresh "the-host-is-never-listed")
  mapM_ (say door (talkWriter stage) (talkChannel stage)) ["one", "two", "three"]
  before <- hear door (talkReader stage) (talkChannel stage)
  forM_ [0 :: Int .. 99] $ \n ->
    plant ground (Address (Text.pack (junkName n))) (ByteString.replicate 131072 (fromIntegral n))
  after <- hear door (talkReader stage) (talkChannel stage)
  pure $
    if before == after
      then Right ()
      else Left ("extra objects on the host changed a read:\n  before: " <> show before <> "\n  after:  " <> show after)
  where
    junkName n = replicate 2 (hexDigit (n `div` 16)) <> replicate 36 (hexDigit (n `mod` 16)) <> "0" <> [hexDigit (n `mod` 13)]
    hexDigit d = "0123456789abcdef" !! (d `mod` 16)

-- | Once the inviter has seen who accepted, the host deleting that acceptance
-- and a second person accepting the same invitation changes nothing: the peer
-- stays who it was, and the impostor's segments never appear.
theFirstPeerIsPinned :: Door -> Ground -> IO (Either String ())
theFirstPeerIsPinned door ground = do
  offered <- Door.ask door (siteOf ground Alice) (Door.Invite (fresh "one-acceptance-only") (waypoint ground) Door.Forever Door.both)
  (channel, invitation) <- case offered of
    Accepted (Invited name line _) -> pure (name, line)
    other -> fail ("the invitation was refused: " <> show other)
  beforeJoin <- map fst <$> stored ground
  joined <- Door.ask door (siteOf ground Bob) (Door.Join invitation channel)
  afterJoin <- map fst <$> stored ground
  pinned <- hear door (siteOf ground Alice) channel
  peerBefore <- peerListed door (siteOf ground Alice) channel
  forM_ [address | address <- afterJoin, address `notElem` beforeJoin] (vanish ground)
  impostor <- Door.ask door (siteOf ground Mallory) (Door.Join invitation channel)
  _ <- Door.ask door (siteOf ground Mallory) (Door.Send channel "impostor")
  _ <- say door (siteOf ground Bob) channel "still bob"
  answer <- hear door (siteOf ground Alice) channel
  peerAfter <- peerListed door (siteOf ground Alice) channel
  pure $ do
    case joined of
      Accepted Joined {} -> Right ()
      other -> Left ("the first acceptance was refused: " <> show other)
    case pinned of
      Accepted Read {} -> Right ()
      other -> Left ("the inviter could not read after the acceptance: " <> show other)
    neverShown ("a second acceptance (" <> show impostor <> ")") "impostor" answer
    if peerAfter == peerBefore
      then Right ()
      else Left ("the listed peer changed from " <> show peerBefore <> " to " <> show peerAfter)

-- | On a releasing channel an acknowledged drop leaves the host, and putting
-- a copy back does not bring it back to either reader.
aReleasedDropIsGoneAndStaysGone :: Door -> Ground -> IO (Either String ())
aReleasedDropIsGoneAndStaysGone door ground = do
  stage <- talkWith door ground Alice Bob (Door.InviteReleasing (fresh "burn-after-reading") (waypoint ground))
  address <- say door (talkWriter stage) (talkChannel stage) "burn me"
  copy <- holding ground address
  _ <- hear door (talkReader stage) (talkChannel stage)
  _ <- say door (talkReader stage) (talkChannel stage) "read it"
  _ <- hear door (talkWriter stage) (talkChannel stage)
  held <- map fst <$> stored ground
  plant ground address copy
  reader <- hear door (talkReader stage) (talkChannel stage)
  writer <- hearMine door (talkWriter stage) (talkChannel stage)
  disks <- concat <$> mapM siteBytes [talkWriter stage, talkReader stage]
  pure $ do
    if address `elem` held
      then Left "the peer acknowledged a drop on a releasing channel and the host still holds it"
      else Right ()
    neverShown "a released drop put back" "burn me" reader
    neverShown "a released drop put back, to its own author" "burn me" writer
    case anyContains "burn me" disks of
      [] -> Right ()
      (path : _) -> Left ("a released message is on disk at " <> path)

-- | A host restored from a backup has the author's own stream shorter than
-- both ends remember. A reader with no memory of the stream is refused, and
-- the reader that remembers still receives the author's next segment: it
-- resumes from its own record, the new segment links to the head it verified,
-- and the one with a memory is right. What must not happen is the author
-- confirming its predecessor first — that names two adjacent addresses to the
-- host on every send, which is the stream's shape given away.
aRolledBackHostStopsTheAuthorToo :: Door -> Ground -> IO (Either String ())
aRolledBackHostStopsTheAuthorToo door ground = do
  stage <- talk door ground Alice Bob (fresh "restored-from-a-backup")
  first <- say door (talkWriter stage) (talkChannel stage) "one"
  backup <- holding ground first
  later <- mapM (say door (talkWriter stage) (talkChannel stage)) ["two", "three"]
  _ <- hear door (talkReader stage) (talkChannel stage)
  mapM_ (vanish ground) later
  plant ground first backup
  author <- Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "four, after the rollback")
  forgetful <- hear door (talkReader stage) (talkChannel stage)
  remembering <- Door.ask door (talkReader stage) (Door.ReadAfter (talkChannel stage) 2)
  pure $ do
    case author of
      Accepted Sent {} -> Right ()
      other -> Left ("after the rollback the author could not write: " <> show other)
    case forgetful of
      Refused _ -> Right ()
      other -> Left ("a reader walking the whole stream did not notice the rollback: " <> show other)
    case remembering of
      Accepted (Read _ _ (Just 3) [Entry 3 (AsText "four, after the rollback")]) -> Right ()
      other -> Left ("the reader that remembers did not receive the segment after the rollback: " <> show other)

-- | The peer a listing names for one channel.
peerListed :: Door -> FilePath -> ChannelName -> IO (Maybe Text)
peerListed door site channel = do
  listing <- Door.ask door site Door.Channels
  pure $ case listing of
    Accepted (Channels summaries) -> case [summaryPeer s | s <- summaries, summaryName s == channel] of
      (peer : _) -> peer
      [] -> Nothing
    _ -> Nothing

-- | The answer never carries this text as a segment, whatever else it does.
neverShown :: String -> Text -> Answer -> Either String ()
neverShown what text = neverCarried what (AsText text)

neverCarried :: String -> Carried -> Answer -> Either String ()
neverCarried what carried answer =
  case answer of
    Refused _ -> Right ()
    Accepted (Read _ _ _ entries)
      | any ((== carried) . entryCarried) entries -> Left (what <> " was read as a segment carrying " <> take 80 (show carried))
      | otherwise -> Right ()
    Accepted other -> Left (what <> " made a read answer with " <> show other)

neverShownBytes :: String -> ByteString -> Answer -> Either String ()
neverShownBytes what bytes = neverCarried what (AsBytes (Text.pack (concatMap hex (ByteString.unpack bytes))))
  where
    hex byte = ["0123456789abcdef" !! fromIntegral (byte `div` 16), "0123456789abcdef" !! fromIntegral (byte `mod` 16)]

boundedBelow :: String -> Word64 -> Answer -> Either String ()
boundedBelow who gap answer =
  case answer of
    Refused _ -> Right ()
    Accepted (Read _ _ (Just height) _) | height >= gap -> Left (who <> " reported height " <> show height <> " across a gap at " <> show gap)
    Accepted Read {} -> Right ()
    Accepted other -> Left (who <> " answered with " <> show other)

atLeast :: String -> Word64 -> Answer -> Either String ()
atLeast who floor' answer =
  case answer of
    Accepted (Read _ _ (Just height) _) | height >= floor' -> Right ()
    Accepted (Read _ _ height _) -> Left (who <> " was talked down to " <> show height)
    Refused _ -> Right ()
    Accepted other -> Left (who <> " answered with " <> show other)
