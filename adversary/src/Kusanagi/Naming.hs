-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | One identity, one name, on both sides of a conversation.
--
-- An identity is a key that signs and a name that is written down, and the two
-- are not the same bytes: a handle is the hash of a verifying key, so that the
-- width of a signature scheme stops at the places a signature is checked. That
-- split is worth exactly as much as the agreement it preserves, and the way it
-- fails is quiet — one path along the door prints a name derived from a key,
-- another prints something it stored earlier, and the two disagree only for
-- people who compare them.
--
-- Nobody compares them by hand. So these are relations between traces:
--
-- * **Both ends agree.** What an endpoint answers to under @identity@ is what
--   its peer reads it under, in the outcome of @join@ and in the outcome of
--   every @read@ afterwards.
-- * **A listing abbreviates the same name.** What @channels@ shows is a prefix
--   of the whole name and not a second opinion about it.
--
-- Neither says what a handle *is*, because that is the shipped code's business
-- and restating it here would make this a second authority. They say that
-- however it is computed, it is computed once.
module Kusanagi.Naming
  ( bothEndsAgreeOnWhoTheOtherIs
  ) where

import Data.Text (Text)
import Data.Text qualified as Text

import Kusanagi.Answer (Answer (..), ChannelName (..), Handle (..), Outcome (..), Summary (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, waypoint)

-- | The channel both endpoints open. Names are local, so one will do.
channel :: ChannelName
channel = ChannelName "peer"

-- | Every name each endpoint produces for the other is the same name.
--
-- The trace is the ordinary one: alice invites, bob joins, each says something
-- and reads the other. Six names come out of it and there are only two
-- identities, so five equalities have to hold — and a build that derived a
-- handle in one place and remembered a key in another would break at least one
-- of them without breaking any message.
bothEndsAgreeOnWhoTheOtherIs :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
bothEndsAgreeOnWhoTheOtherIs door ground alice bob = do
  named <- traverse (whoAmI door) [alice, bob]
  case named of
    [Right alicesName, Right bobsName] -> do
      minted <- Door.ask door alice (Door.Invite channel (waypoint ground) Door.Forever Door.both)
      case minted of
        Accepted (Invited _ invitation _) -> do
          joined <- Door.ask door bob (Door.Join invitation channel)
          case joined of
            Accepted (Joined _ bobsOwn alicesAsSeen) -> do
              _ <- Door.ask door alice (Door.Send channel "from alice")
              _ <- Door.ask door bob (Door.Send channel "from bob")
              bobsAsSeen <- authorSeenBy door alice
              alicesAsRead <- authorSeenBy door bob
              listedByAlice <- peerListedBy door alice
              listedByBob <- peerListedBy door bob
              pure $
                allOf
                  [ same "bob's own name" "the name bob answers to" (Right bobsOwn) bobsName
                  , same "the name bob reads alice under" "alice's own name" (Right alicesAsSeen) alicesName
                  , same "the author alice reads on bob's stream" "bob's own name" bobsAsSeen bobsName
                  , same "the author bob reads on alice's stream" "alice's own name" alicesAsRead alicesName
                  , abbreviates "alice's listing of bob" listedByAlice bobsName
                  , abbreviates "bob's listing of alice" listedByBob alicesName
                  ]
            other -> pure (Left ("the channel could not be joined: " <> show other))
        other -> pure (Left ("the invitation was refused: " <> show other))
    other -> pure (Left ("an endpoint could not name itself: " <> show other))

-- | The name an endpoint answers to.
whoAmI :: Door -> FilePath -> IO (Either String Handle)
whoAmI door site = do
  answered <- Door.ask door site Door.Identity
  pure $ case answered of
    Accepted (Identity name) -> Right name
    other -> Left (show other)

-- | The author of the stream this endpoint reads on its one channel.
authorSeenBy :: Door -> FilePath -> IO (Either String Handle)
authorSeenBy door site = do
  answered <- Door.ask door site (Door.Read channel)
  pure $ case answered of
    Accepted (Read _ author _ _) -> Right author
    other -> Left (show other)

-- | What this endpoint's listing shows for the peer of its one channel.
peerListedBy :: Door -> FilePath -> IO (Either String Text)
peerListedBy door site = do
  answered <- Door.ask door site Door.Channels
  pure $ case answered of
    Accepted (Channels [summary]) ->
      maybe (Left "a listing showed no peer after both ends had spoken") Right (summaryPeer summary)
    other -> Left (show other)

-- | Two names that must be one name.
same :: String -> String -> Either String Handle -> Handle -> Either String ()
same leftName rightName left right = do
  Handle found <- left
  let Handle wanted = right
  if found == wanted
    then Right ()
    else
      Left
        ( leftName
            <> " is "
            <> Text.unpack found
            <> ", and "
            <> rightName
            <> " is "
            <> Text.unpack wanted
            <> "; one identity is answering to two names"
        )

-- | A listing shows a shortened name and never a different one.
abbreviates :: String -> Either String Text -> Handle -> Either String ()
abbreviates what shown (Handle whole) = do
  found <- shown
  if found `Text.isPrefixOf` whole && not (Text.null found)
    then Right ()
    else
      Left
        ( what
            <> " shows "
            <> Text.unpack found
            <> ", which is not the start of "
            <> Text.unpack whole
        )

-- | Every reason, or none.
allOf :: [Either String ()] -> Either String ()
allOf results = case [reason | Left reason <- results] of
  [] -> Right ()
  reasons -> Left (unlines reasons)
