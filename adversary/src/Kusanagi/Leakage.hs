-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What three adversaries can grep for, and must not find.
--
-- The host holds every object. A second account, a thief, or a subpoena holds
-- a site's directory — its bytes and, separately, its file names, because a
-- listing is readable by anybody the bytes are not. Whoever finds an archive
-- holds that. Each of them is handed a list of needles that identify somebody
-- or something said, and each property is one sentence: none of the needles
-- is anywhere in what that adversary has.
--
-- Needles come in both shapes a leak could take — the rendering a person sees
-- and the raw bytes behind it — because a record that stores a handle as 32
-- bytes leaks it exactly as much as one that stores 64 hexadecimal digits.
module Kusanagi.Leakage
  ( hostHoldsNoWord
  , twoChannelsShareNothing
  , twoHostsShareNothing
  , theSiteHoldsNoMessage
  , theSiteHoldsNoName
  , noFileIsNamedAfterAnybody
  , noTwoFilesShareAName
  , twoSitesShareNoFilename
  , theArchiveIsOpaque
  , theSecretIsSaidOnce
  ) where

import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.List (intersect, nub, sort, tails)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Directory (createDirectoryIfMissing)
import System.FilePath (splitDirectories, takeDirectory, (</>))
import System.Info (os)

import Kusanagi.Answer (Address (..), ChannelName (..), Handle (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Service qualified as Service
import Kusanagi.Ground (Ground, Site (..), siteOf, stored, waypoint)
import Kusanagi.Stage
import Kusanagi.Veil (apart, pairs)

-- | Two sentences nobody would say by chance, in both directions.
saidByWriter, saidByReader :: Text
saidByWriter = "the quarterly numbers are seventeen million and falling"
saidByReader = "wire the retainer before noon on thursday"

-- | The host's objects contain nothing said, nobody's name, and no secret.
hostHoldsNoWord :: Door -> Ground -> IO (Either String ())
hostHoldsNoWord door ground = do
  stage <- talk door ground Alice Bob (fresh "quarterly-numbers-2026")
  exchange door stage
  held <- stored ground
  pure (nowhere "an object the host holds" (needles stage) [(Text.unpack address, bytes) | (Address address, bytes) <- held])

-- | One identity on two channels leaves the host objects that pair off with
-- nothing: neither channel's drops look like the other's.
twoChannelsShareNothing :: Door -> Ground -> IO (Either String ())
twoChannelsShareNothing door ground = do
  first <- talk door ground Alice Bob (fresh "with-bob")
  second <- talk door ground Alice Mallory (fresh "with-mallory")
  mapM_ (exchange door) [first, second]
  held <- stored ground
  pure (allApart (map snd held))

-- | One identity on two hosts: the addresses share no prefix and the bodies
-- share no structure, so two hosts comparing notes learn nothing.
twoHostsShareNothing :: Door -> Ground -> IO (Either String ())
twoHostsShareNothing door ground = do
  let second = takeDirectory (waypoint ground) </> "second-host"
  createDirectoryIfMissing True second
  near <- talk door ground Alice Bob (fresh "on-the-first-host")
  far <- talkWith door ground Alice Mallory (Door.Invite (fresh "on-the-second-host") second Door.Forever Door.both)
  mapM_ (exchange door) [near, far]
  nearHeld <- stored ground
  farHeld <- siteBytes second
  let nearNames = [Text.unpack address | (Address address, _) <- nearHeld]
      farNames = [concat (drop (length parts - 2) parts) | (path, _) <- farHeld, let parts = splitDirectories path]
      prefixes = [take 8 a | a <- nearNames, b <- farNames, take 8 a == take 8 b]
  pure $ case prefixes of
    (shared : _) -> Left ("an address on each host begins with " <> shared)
    [] -> allApart (map snd nearHeld <> map snd farHeld)

-- | A site keeps no message, on every platform.
theSiteHoldsNoMessage :: Door -> Ground -> IO (Either String ())
theSiteHoldsNoMessage door ground = do
  stage <- talk door ground Alice Bob (fresh "nothing-kept-here")
  exchange door stage
  held <- mapM siteBytes [talkWriter stage, talkReader stage]
  pure (nowhere "a file in a site" (spoken stage) (concat held))

-- | A site's bytes name no channel, no handle and no secret.
--
-- Only where the platform seals records: elsewhere a record is plain bytes
-- under mode bits, and full-disk encryption is the stated premise (D-04).
theSiteHoldsNoName :: Door -> Ground -> IO (Either String ())
theSiteHoldsNoName door ground
  | os /= "mingw32" = pure (Right ())
  | otherwise = do
      stage <- talk door ground Alice Bob (fresh "sealed-at-rest-here")
      exchange door stage
      held <- mapM siteBytes [talkWriter stage, talkReader stage]
      pure (nowhere "a file in a site" (identifying stage) (concat held))

-- | No path component under a site carries a channel name or eight digits of
-- anybody's handle. A listing is the one thing a second account always gets.
noFileIsNamedAfterAnybody :: Door -> Ground -> IO (Either String ())
noFileIsNamedAfterAnybody door ground = do
  stage <- talk door ground Alice Bob (fresh "named-after-nobody")
  exchange door stage
  names <- concat <$> mapM siteNames [talkWriter stage, talkReader stage]
  let ChannelName channel = talkChannel stage
      slices = nub (concatMap eights [talkWriterHandle stage, talkReaderHandle stage])
      guilty = [name | name <- names, Text.unpack channel `isIn` name || any (`isIn` name) slices]
  pure $ case guilty of
    [] -> Right ()
    (name : _) -> Left ("a file is named after somebody: " <> name)
  where
    eights (Handle rendered) = [take 8 slice | slice <- tails (Text.unpack rendered), length slice >= 8]
    isIn needle haystack = any (\slice -> take (length needle) slice == needle) (tails haystack)

-- | Two channels with the same peer leave no file name in common, so a
-- listing gives up a count of channels and not which of them share a person.
noTwoFilesShareAName :: Door -> Ground -> IO (Either String ())
noTwoFilesShareAName door ground = do
  first <- talk door ground Alice Bob (fresh "first-with-the-same-peer")
  second <- talk door ground Alice Bob (fresh "second-with-the-same-peer")
  mapM_ (exchange door) [first, second]
  files <- map fst <$> siteBytes (talkWriter first)
  let names = sort (map baseName files)
      repeated = [a | (a, b) <- zip names (drop 1 names), a == b]
  pure $ case repeated of
    [] -> Right ()
    (name : _) -> Left ("two files in one site are both called " <> name)

-- | Two sites that both talk to Bob have no file name in common beyond what a
-- site that talks to nobody has, so two seized disks cannot be joined on one.
twoSitesShareNoFilename :: Door -> Ground -> IO (Either String ())
twoSitesShareNoFilename door ground = do
  first <- talk door ground Alice Bob (fresh "alice-and-bob")
  second <- talk door ground Mallory Bob (fresh "mallory-and-bob")
  mapM_ (exchange door) [first, second]
  let solo = siteOf ground Bob </> "nobody"
  _ <- Door.ask door solo Door.Identity
  [alice, mallory, lonely] <- mapM (fmap (map (baseName . fst)) . siteBytes) [talkWriter first, talkWriter second, solo]
  let shared = nub (alice `intersect` mallory)
      unexplained = [name | name <- shared, name `notElem` lonely]
  pure $ case unexplained of
    [] -> Right ()
    (name : _) -> Left ("two sites that share a peer both hold a file called " <> name)

-- | An archive without its key says nothing, and never the key itself.
theArchiveIsOpaque :: Door -> Ground -> IO (Either String ())
theArchiveIsOpaque door ground = do
  stage <- talk door ground Alice Bob (fresh "sealed-into-an-archive")
  exchange door stage
  sealed <- Service.exporting door (talkWriter stage)
  pure $ case sealed of
    Left complaint -> Left ("export was refused: " <> show complaint)
    Right (key, archive) ->
      nowhere "the archive" (needles stage <> hexOf key) [("archive", archive)]

-- | The secret half of an invitation is printed by `invite` and never again.
theSecretIsSaidOnce :: Door -> Ground -> IO (Either String ())
theSecretIsSaidOnce door ground = do
  stage <- talk door ground Alice Bob (fresh "said-once-only")
  exchange door stage
  let ChannelName channel = talkChannel stage
      line = Text.encodeUtf8 (channel <> "\n")
      at site arguments input = Door.typed door (["--root", site, "--json"] <> arguments) input
  outputs <-
    sequence
      [ at (talkWriter stage) ["channels"] Nothing
      , at (talkWriter stage) ["read", "--from", "-"] (Just line)
      , at (talkWriter stage) ["read", "--from", "-", "--mine"] (Just line)
      , at (talkWriter stage) ["export"] Nothing
      , at (talkReader stage) ["channels"] Nothing
      , at (talkReader stage) ["id"] Nothing
      , at (talkReader stage) ["forget", "--channel", "-"] (Just line)
      , at (talkReader stage) ["read", "--from", "-"] (Just line)
      ]
  let streams = concat [[("stdout", Door.typedOut t), ("stderr", Door.typedErr t)] | t <- outputs]
  pure (nowhere "an output after invite" (secretOf (talkInvitation stage)) streams)

-- | Says one thing each way and reads both, so that cairns and records exist.
exchange :: Door -> Talk -> IO ()
exchange door stage = do
  _ <- say door (talkWriter stage) (talkChannel stage) saidByWriter
  _ <- say door (talkReader stage) (talkChannel stage) saidByReader
  mapM_ (\site -> hear door site (talkChannel stage)) [talkWriter stage, talkReader stage]

-- | Everything that identifies a party or a secret of this conversation.
identifying :: Talk -> [ByteString]
identifying stage =
  let ChannelName channel = talkChannel stage
   in Text.encodeUtf8 channel
        : handleOf (talkWriterHandle stage)
          <> handleOf (talkReaderHandle stage)
          <> secretOf (talkInvitation stage)

-- | What was said, in both directions.
spoken :: Talk -> [ByteString]
spoken _ = map Text.encodeUtf8 [saidByWriter, saidByReader]

needles :: Talk -> [ByteString]
needles stage = spoken stage <> identifying stage

-- | No needle in any haystack, or which one was where.
nowhere :: String -> [ByteString] -> [(FilePath, ByteString)] -> Either String ()
nowhere place wanted held =
  case [(needle, path) | needle <- wanted, path <- anyContains needle held] of
    [] -> Right ()
    ((needle, path) : _) ->
      Left (place <> " contains " <> show (ByteString.take 48 needle) <> ": " <> path)

allApart :: [ByteString] -> Either String ()
allApart bodies =
  case [reason | (left, right) <- pairs bodies, Just reason <- [apart left right]] of
    [] -> Right ()
    (reason : _) -> Left reason

baseName :: FilePath -> FilePath
baseName = reverse . takeWhile (\c -> c /= '\\' && c /= '/') . reverse
