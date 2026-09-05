-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | Two endpoints put in conversation, and what each party to the threat model
-- is then holding.
--
-- Every property in the surface matrix begins the same way — somebody invites,
-- somebody joins, something is said — and then takes the position of one
-- adversary and looks at what that adversary has: the host's objects, a site's
-- bytes, a site's file names, an archive. This module is that beginning and
-- those positions, so that a property is only the relation it asserts.
--
-- Nothing here judges. A `Left` from 'talk' means the stage could not be set,
-- which is a broken world rather than a finding.
module Kusanagi.Stage
  ( Talk (..)
  , talk
  , talkWith
  , say
  , hear
  , hearMine
  , entriesOf
  , siteBytes
  , siteNames
  , contains
  , anyContains
  , hexOf
  , handleOf
  , secretOf
  , fresh
  ) where

import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Directory (doesDirectoryExist, listDirectory)
import System.FilePath (takeFileName, (</>))

import Kusanagi.Answer
  ( Address
  , Answer (..)
  , ChannelName (..)
  , Entry
  , Handle (..)
  , Invitation (..)
  , Outcome (..)
  )
import Kusanagi.Door (Door, Verb)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site, siteOf, waypoint)

-- | One channel between two sites, and what both ends said about it.
data Talk = Talk
  { talkWriter :: FilePath
  , talkReader :: FilePath
  , talkChannel :: ChannelName
  , talkInvitation :: Invitation
  , -- | The writer's handle, as the reader's `joined` reported it.
    talkWriterHandle :: Handle
  , -- | The reader's handle, as its own `joined` reported it.
    talkReaderHandle :: Handle
  }
  deriving stock (Eq, Show)

-- | Opens an on-demand channel from one site to another on the ground's host.
talk :: Door -> Ground -> Site -> Site -> ChannelName -> IO Talk
talk door ground writer reader channel =
  talkWith door ground writer reader (Door.Invite channel (waypoint ground) Door.Forever Door.both)

-- | Opens a channel with whatever invitation the caller minted.
--
-- The verb must be an invitation on @channel@; the reader joins under the same
-- local name, which keeps every property free of a second name to track.
talkWith :: Door -> Ground -> Site -> Site -> Verb -> IO Talk
talkWith door ground writer reader minting = do
  minted <- Door.ask door (siteOf ground writer) minting
  (channel, invitation) <- case minted of
    Accepted (Invited name line _) -> pure (name, line)
    other -> fail ("the invitation was refused: " <> show other)
  joined <- Door.ask door (siteOf ground reader) (Door.Join invitation channel)
  case joined of
    Accepted (Joined _ own peer) ->
      pure
        Talk
          { talkWriter = siteOf ground writer
          , talkReader = siteOf ground reader
          , talkChannel = channel
          , talkInvitation = invitation
          , talkWriterHandle = peer
          , talkReaderHandle = own
          }
    other -> fail ("the channel could not be joined: " <> show other)

-- | Says one thing from one site, and reports where the host was told to put it.
say :: Door -> FilePath -> ChannelName -> Text -> IO Address
say door site channel text = do
  said <- Door.ask door site (Door.Send channel text)
  case said of
    Accepted (Sent _ _ address) -> pure address
    other -> fail ("a segment was refused: " <> show other)

-- | Reads the peer's stream from one site, and hands back the whole answer.
hear :: Door -> FilePath -> ChannelName -> IO Answer
hear door site channel = Door.ask door site (Door.Read channel)

-- | Reads this site's own stream, as the peer would.
hearMine :: Door -> FilePath -> ChannelName -> IO Answer
hearMine door site channel = Door.ask door site (Door.ReadMine channel)

-- | The entries of a read, or why there were none.
entriesOf :: Answer -> Either String [Entry]
entriesOf (Accepted (Read _ _ _ entries)) = Right entries
entriesOf other = Left ("a read answered with something else: " <> show other)

-- | Every file under a site, with its bytes.
--
-- What a second account, a thief with the disk, or a subpoena is holding: all
-- of it, whatever the layout, so that a property does not have to know where
-- anything is kept.
siteBytes :: FilePath -> IO [(FilePath, ByteString)]
siteBytes root = do
  there <- doesDirectoryExist root
  if there then walk root else pure []
  where
    walk directory = do
      entries <- listDirectory directory
      fmap concat . mapM (visit directory) $ entries
    visit directory entry = do
      let path = directory </> entry
      isDirectory <- doesDirectoryExist path
      if isDirectory
        then walk path
        else do
          bytes <- ByteString.readFile path
          pure [(path, bytes)]

-- | Every path component under a site, which is what a listing gives away.
siteNames :: FilePath -> IO [FilePath]
siteNames root = do
  there <- doesDirectoryExist root
  if there then walk root else pure []
  where
    walk directory = do
      entries <- listDirectory directory
      fmap concat . mapM (visit directory) $ entries
    visit directory entry = do
      let path = directory </> entry
      isDirectory <- doesDirectoryExist path
      below <- if isDirectory then walk path else pure []
      pure (takeFileName path : below)

-- | Whether a needle occurs anywhere in a haystack of bytes.
contains :: ByteString -> ByteString -> Bool
contains needle haystack = not (ByteString.null needle) && ByteString.isInfixOf needle haystack

-- | Which of the named haystacks hold the needle.
anyContains :: ByteString -> [(FilePath, ByteString)] -> [FilePath]
anyContains needle held = [path | (path, bytes) <- held, contains needle bytes]

-- | Lowercase hexadecimal as bytes, and its raw decoding, both of which a
-- leak could take the shape of.
hexOf :: Text -> [ByteString]
hexOf rendered = [Text.encodeUtf8 rendered, unhex (Text.encodeUtf8 rendered)]
  where
    unhex bytes = ByteString.pack (go (ByteString.unpack bytes))
    go (high : low : rest) = nibble high * 16 + nibble low : go rest
    go _ = []
    nibble c
      | c >= 48 && c <= 57 = c - 48
      | c >= 97 && c <= 102 = c - 87
      | otherwise = 0

-- | Both shapes a handle can leak in.
handleOf :: Handle -> [ByteString]
handleOf (Handle rendered) = hexOf rendered

-- | Both shapes the secret half of an invitation can leak in.
--
-- The payload after the scheme is a suite byte, a version byte, the 64 secret
-- bytes, and then the locator in the clear. Asserting that the whole payload
-- never appears is weaker than it looks — the locator is public — so this
-- hands back the secret's own 128 digits and a 32-digit slice from inside them,
-- which is what a partial leak would still contain.
secretOf :: Invitation -> [ByteString]
secretOf (Invitation line) =
  concatMap hexOf [secret, Text.take 32 (Text.drop 40 secret)]
  where
    payload = Text.drop 1 (Text.dropWhile (/= ':') line)
    secret = Text.take 128 (Text.drop 4 payload)

-- | A channel name nobody else on the stage uses.
fresh :: Text -> ChannelName
fresh = ChannelName
