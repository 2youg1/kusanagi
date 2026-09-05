-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What a member of a group holds, and what an ex-peer is still sent.
--
-- A small group here is one endpoint's private roster and one drop per
-- member, so the promise to each member is that the others do not exist as
-- far as their own disk, their own reads and the host's objects can show.
-- Revocation is the other edge of the same promise: a member cut off must
-- stop receiving, not merely stop being read, or a fan-out keeps leaking to
-- the person it was meant to exclude.
module Kusanagi.Insider
  ( aMemberLearnsNoOtherMember
  , aBroadcastLooksLikeAWhisper
  , aBroadcastIsTwoStrangers
  , aRevokedMemberIsLeftOut
  , sendingToTheRevokedFailsLikeReadingThem
  ) where

import Data.Aeson (Value (..), decodeStrict')
import Data.Aeson.KeyMap qualified as KeyMap
import Data.List (sort)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text

import Kusanagi.Answer
  ( Answer (..)
  , Carried (..)
  , ChannelName (..)
  , Complaint (..)
  , Entry (..)
  , Landed (..)
  , Outcome (..)
  )
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..), stored)
import Kusanagi.Stage
import Kusanagi.Veil (apart)

-- | Alice, a roster of Bob and Mallory, and one sentence to both.
data Team = Team
  { withBob :: Talk
  , withMallory :: Talk
  , team :: ChannelName
  , landed :: [Landed]
  }

assemble :: Door -> Ground -> IO Team
assemble door ground = do
  bob <- talk door ground Alice Bob (fresh "with-bob-only")
  mallory <- talk door ground Alice Mallory (fresh "with-mallory-only")
  let name = fresh "the-whole-team"
  grouped <- Door.ask door (talkWriter bob) (Door.Group name [talkChannel bob, talkChannel mallory])
  case grouped of
    Accepted Grouped {} -> pure ()
    other -> fail ("the group was refused: " <> show other)
  fanned <- Door.ask door (talkWriter bob) (Door.SendGroup name "the quarterly numbers, to everybody")
  -- Reading once is how the inviter learns who accepted; revoking needs that.
  mapM_ (\stage -> hear door (talkWriter stage) (talkChannel stage)) [bob, mallory]
  case fanned of
    Accepted (FannedOut _ where') -> pure (Team bob mallory name where')
    other -> fail ("the fan-out was refused: " <> show other)

-- | Bob hands over his disk and everything he can read: Mallory is in none of it.
aMemberLearnsNoOtherMember :: Door -> Ground -> IO (Either String ())
aMemberLearnsNoOtherMember door ground = do
  squad <- assemble door ground
  let bob = talkReader (withBob squad)
      ChannelName channel = talkChannel (withBob squad)
      ChannelName mallorysChannel = talkChannel (withMallory squad)
      ChannelName groupName = team squad
  reading <- Door.typed door ["--root", bob, "--json", "read", "--from", "-"] (Just (Text.encodeUtf8 (channel <> "\n")))
  listing <- Door.typed door ["--root", bob, "--json", "channels"] Nothing
  disk <- siteBytes bob
  names <- siteNames bob
  held <- stored ground
  let needles = handleOf (talkReaderHandle (withMallory squad)) <> map Text.encodeUtf8 [mallorysChannel, groupName]
      haystacks =
        disk
          <> [("read", Door.typedOut reading), ("channels", Door.typedOut listing)]
          <> [("a file name", Text.encodeUtf8 (Text.pack name)) | name <- names]
          <> [("host object", bytes) | (_, bytes) <- held]
  pure $ case [(needle, place) | needle <- needles, place <- anyContains needle haystacks] of
    [] -> Right ()
    ((needle, place) : _) -> Left ("a member can find " <> show needle <> " in " <> place)

-- | A segment that was fanned out has the same JSON keys as one that was not.
aBroadcastLooksLikeAWhisper :: Door -> Ground -> IO (Either String ())
aBroadcastLooksLikeAWhisper door ground = do
  squad <- assemble door ground
  let bob = talkReader (withBob squad)
      ChannelName channel = talkChannel (withBob squad)
  _ <- say door (talkWriter (withBob squad)) (talkChannel (withBob squad)) "to bob alone"
  reading <- Door.typed door ["--root", bob, "--json", "read", "--from", "-"] (Just (Text.encodeUtf8 (channel <> "\n")))
  pure $ case decodeStrict' (Door.typedOut reading) of
    Just (Object answer)
      | Just (Array segments) <- KeyMap.lookup "segments" answer ->
          case [sort (KeyMap.keys o) | Object o <- foldr (:) [] segments] of
            [broadcast, whisper]
              | broadcast == whisper -> Right ()
              | otherwise -> Left ("a broadcast carries keys " <> show broadcast <> " and a whisper " <> show whisper)
            other -> Left ("expected two segments, saw " <> show (length other))
    _ -> Left "the read did not parse as an object with segments"

-- | The two drops of one sentence share nothing on the host.
aBroadcastIsTwoStrangers :: Door -> Ground -> IO (Either String ())
aBroadcastIsTwoStrangers door ground = do
  squad <- assemble door ground
  held <- stored ground
  let addresses = [address | Landed _ "sent" _ (Just address) <- landed squad]
      bodies = [bytes | (address, bytes) <- held, address `elem` addresses]
  pure $ case bodies of
    [left, right] -> maybe (Right ()) Left (apart left right)
    _ -> Left ("the fan-out named " <> show (length addresses) <> " addresses and the host holds " <> show (length bodies) <> " of them")

-- | After Bob is revoked, a fan-out lands on Mallory and not on Bob, says so
-- with a code, and Bob's next read does not carry the sentence.
aRevokedMemberIsLeftOut :: Door -> Ground -> IO (Either String ())
aRevokedMemberIsLeftOut door ground = do
  squad <- assemble door ground
  let alice = talkWriter (withBob squad)
  revoked <- Door.ask door alice (Door.Revoke (talkChannel (withBob squad)))
  fanned <- Door.ask door alice (Door.SendGroup (team squad) "after bob was cut off")
  bobHears <- hear door (talkReader (withBob squad)) (talkChannel (withBob squad))
  malloryHears <- hear door (talkReader (withMallory squad)) (talkChannel (withMallory squad))
  pure $ do
    case revoked of
      Accepted Revoked {} -> Right ()
      other -> Left ("revoke answered " <> show other)
    case fanned of
      Accepted (FannedOut _ where') -> do
        case [l | l <- where', landedMember l == talkChannel (withBob squad)] of
          [Landed _ status code _] | status /= "sent", code /= Nothing -> Right ()
          other -> Left ("the fan-out reported the revoked member as " <> show other)
        case [l | l <- where', landedMember l == talkChannel (withMallory squad)] of
          [Landed _ "sent" _ _] -> Right ()
          other -> Left ("the fan-out reported the remaining member as " <> show other)
      Refused complaint -> Left ("a fan-out with one revoked member was refused outright: " <> show complaint)
      Accepted other -> Left ("the fan-out answered " <> show other)
    case entriesOf bobHears of
      Right entries | "after bob was cut off" `notElem` map shown entries -> Right ()
      Right _ -> Left "the revoked member received the fan-out"
      Left _ -> Right ()
    case entriesOf malloryHears of
      Right entries | "after bob was cut off" `elem` map shown entries -> Right ()
      other -> Left ("the remaining member did not receive the fan-out: " <> show other)
  where
    shown entry = case entryCarried entry of
      AsText text -> text
      AsBytes hex -> hex

-- | Sending on a channel whose peer was revoked fails with the same code as
-- reading one, so a revocation is one fact and not a reading-only fact.
sendingToTheRevokedFailsLikeReadingThem :: Door -> Ground -> IO (Either String ())
sendingToTheRevokedFailsLikeReadingThem door ground = do
  stage <- talk door ground Alice Bob (fresh "cut-off-both-ways")
  _ <- say door (talkReader stage) (talkChannel stage) "before"
  _ <- hear door (talkWriter stage) (talkChannel stage)
  revoked <- Door.ask door (talkWriter stage) (Door.Revoke (talkChannel stage))
  reading <- hear door (talkWriter stage) (talkChannel stage)
  sending <- Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "after")
  pure $ case (revoked, reading, sending) of
    (Refused complaint, _, _) -> Left ("the inviter could not revoke: " <> show complaint)
    (Accepted _, Refused r, Refused s)
      | complaintCode r == complaintCode s -> Right ()
      | otherwise -> Left ("reading a revoked peer fails with " <> show (complaintCode r) <> " and sending to one with " <> show (complaintCode s))
    (Accepted _, Refused _, Accepted outcome) -> Left ("a revoked peer can still be sent to: " <> show outcome)
    (Accepted _, other, _) -> Left ("reading a revoked peer answered " <> show other)
