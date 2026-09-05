-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TupleSections #-}

-- | A room: every member sweeps one ward, every member's stream is read.
--
-- Two claims, and they are the two halves of the price D-17 wrote down.
-- A member who hands over their disk and their reads hands over every other
-- member's handle — that is the cost, stated so that nobody mistakes a room
-- for a fan-out. The host that holds every drop of the room holds no handle,
-- no name and no sentence — that is what the cost buys.
module Kusanagi.Room
  ( aMemberCanListEveryMember
  , theHostHoldsNoMember
  ) where

import Data.Aeson (Key, Value (..), decodeStrict')
import Data.Aeson.KeyMap qualified as KeyMap
import Data.ByteString (ByteString)
import Data.List (sort)
import Data.Text (Text)
import Data.Text.Encoding qualified as Text
import Data.Vector qualified as Vector
import System.Exit (ExitCode (..))

import Kusanagi.Answer (Answer (..), Handle (..), Outcome (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..), cast, siteOf, stored, waypoint)
import Kusanagi.Stage (anyContains, handleOf)

-- | The room every property here starts from: founded by Alice, joined by Bob
-- and Mallory, admitted on Alice's first read, one sentence from each.
data Squad = Squad
  { handles :: [(Site, Handle)]
  , said :: [Text]
  }

room :: ByteString
room = "team\n"

assemble :: Door -> Ground -> IO Squad
assemble door ground = do
  founded <- Door.typed door (argv Alice ["room", "--name", "-", "--waypoint", waypoint ground]) (Just room)
  accepted "founding the room" founded
  mapM_ admit [Bob, Mallory]
  -- The founder's read is what admits: it reads each introduction stream,
  -- re-signs the roster once, and carries it on her own stream.
  Door.typed door (argv Alice ["room-read", "--name", "-"]) (Just room) >>= accepted "the founder's first read"
  mapM_ (\(site, sentence) -> Door.typed door (argv site ["room-send", "--name", "-"]) (Just (room <> Text.encodeUtf8 sentence)) >>= accepted "a room send") (zip cast sentences)
  named <- mapM (\site -> (site,) <$> handleAt site) cast
  pure (Squad named sentences)
  where
    argv site rest = ["--root", siteOf ground site, "--json"] <> rest
    admit site = do
      invited <- Door.typed door (argv Alice ["room-invite", "--name", "-", "--for", "3600"]) (Just room)
      accepted "minting a room invitation" invited
      line <- maybe (fail "the invitation carried no line") pure (field "invite" (Door.typedOut invited))
      Door.typed door (argv site ["room-join", "--name", "-"]) (Just (room <> Text.encodeUtf8 line)) >>= accepted "joining the room"
    handleAt site = do
      answer <- Door.ask door (siteOf ground site) Door.Identity
      case answer of
        Accepted (Identity handle) -> pure handle
        other -> fail ("no identity: " <> show other)
    sentences = ["alice on the ledger", "bob on the shortfall", "mallory on the auditors"]

-- | Bob reads the room and can name every other member by handle. That is the
-- price of a room, and it is paid in the open: the reader is told who wrote.
aMemberCanListEveryMember :: Door -> Ground -> IO (Either String ())
aMemberCanListEveryMember door ground = do
  squad <- assemble door ground
  reading <- Door.typed door ["--root", siteOf ground Bob, "--json", "room-read", "--name", "-"] (Just room)
  pure $ case authors (Door.typedOut reading) of
    Nothing -> Left ("bob's read did not parse as a room: " <> show (Door.typedOut reading))
    Just named
      | sort named == sort [rendered | (_, Handle rendered) <- handles squad] -> Right ()
      | otherwise -> Left ("bob's read names " <> show named <> " and the room holds " <> show (handles squad))

-- | The host holds every drop of the room and can find in them no member's
-- handle, no room name, and no sentence said.
theHostHoldsNoMember :: Door -> Ground -> IO (Either String ())
theHostHoldsNoMember door ground = do
  squad <- assemble door ground
  held <- stored ground
  let objects = [(show address, bytes) | (address, bytes) <- held]
      needles =
        concatMap (handleOf . snd) (handles squad)
          <> map Text.encodeUtf8 (said squad)
          <> ["team"]
  pure $ case [(needle, place) | needle <- needles, place <- anyContains needle objects] of
    [] | length objects >= 3 -> Right ()
    [] -> Left ("the host holds " <> show (length objects) <> " objects; a room of three said three things")
    ((needle, place) : _) -> Left ("the host can find " <> show needle <> " in " <> place)

accepted :: String -> Door.Typed -> IO ()
accepted what typed
  | Door.typedStatus typed == ExitSuccess = pure ()
  | otherwise = fail (what <> " was refused: " <> show (Door.typedOut typed) <> show (Door.typedErr typed))

field :: Key -> ByteString -> Maybe Text
field key bytes = case decodeStrict' bytes of
  Just (Object o) | Just (String value) <- KeyMap.lookup key o -> Just value
  _ -> Nothing

authors :: ByteString -> Maybe [Text]
authors bytes = case decodeStrict' bytes of
  Just (Object o) | Just (Array threads) <- KeyMap.lookup "threads" o ->
    Just [author | Object thread <- Vector.toList threads, Just (String author) <- [KeyMap.lookup "author" thread]]
  _ -> Nothing
