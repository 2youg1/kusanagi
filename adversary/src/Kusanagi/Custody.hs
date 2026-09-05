-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What a site keeps, gives back, lets go of — and how little it trusts its
-- own disk.
--
-- Zero trust does not stop at the network. A site's directory is written by
-- this program and read by this program, and that is exactly the kind of
-- provenance an attacker with write access, a failing disk or a half-finished
-- copy forges for free. So the disk is treated like the host: every file may
-- be wrong, and being wrong must produce a coded refusal or the same answer,
-- never a different answer and never a new identity.
module Kusanagi.Custody
  ( importRefusesWhatIsNotItsKey
  , aRestoredSiteReadsTheSame
  , forgettingLeavesNothingBehind
  , nothingIsLeftHalfWritten
  , aRefusedVerbChangesNothingOnDisk
  , aCorruptedFileNeverChangesWhoYouAre
  ) where

import Control.Monad (forM)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.List (isPrefixOf, isSuffixOf)
import Data.Text qualified as Text
import System.Directory (createDirectoryIfMissing, doesDirectoryExist, listDirectory)
import System.FilePath (takeFileName, (</>))

import Kusanagi.Answer (Answer (..), Code (..), Complaint (..), Outcome (..), Summary (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Service qualified as Service
import Kusanagi.Ground (Ground, Site (..), siteOf, stored, waypoint)
import Kusanagi.Stage

-- | A wrong key, a damaged archive and an occupied root are each refused, and
-- a refused import leaves the root as empty as it found it.
importRefusesWhatIsNotItsKey :: Door -> Ground -> IO (Either String ())
importRefusesWhatIsNotItsKey door ground = do
  stage <- talk door ground Alice Bob (fresh "to-be-archived")
  _ <- say door (talkWriter stage) (talkChannel stage) "kept for later"
  sealed <- Service.exporting door (talkWriter stage)
  case sealed of
    Left complaint -> pure (Left ("export was refused: " <> show complaint))
    Right (key, archive) -> do
      let elsewhere name = siteOf ground Mallory </> name
          wrongKey = Text.map (\c -> if c == '0' then '1' else '0') key
          damaged = flipAt (ByteString.length archive `div` 2) archive
      findings <-
        sequence
          [ refusedAndEmpty "a wrong key" (elsewhere "wrong-key") (Door.Import wrongKey archive)
          , refusedAndEmpty "a malformed key" (elsewhere "bad-key") (Door.Import "not-a-key" archive)
          , refusedAndEmpty "a damaged archive" (elsewhere "damaged") (Door.Import key damaged)
          , refusedAndEmpty "a truncated archive" (elsewhere "short") (Door.Import key (ByteString.take 100 archive))
          , refusedOnly "an occupied root" (talkReader stage) (Door.Import key archive)
          ]
      pure (sequence_ findings)
  where
    refusedAndEmpty what root verb = do
      answer <- Door.ask door root verb
      there <- doesDirectoryExist root
      files <- if there then siteBytes root else pure []
      pure $ case answer of
        Refused complaint
          | null files -> coded what complaint
          | otherwise -> Left (what <> " was refused but left " <> show (length files) <> " file(s) behind")
        Accepted outcome -> Left (what <> " was accepted: " <> show outcome)
    refusedOnly what root verb = do
      answer <- Door.ask door root verb
      pure $ case answer of
        Refused complaint -> coded what complaint
        Accepted outcome -> Left (what <> " was accepted: " <> show outcome)

-- | A site restored from its archive into an empty root reads exactly what the
-- original read: the same entries, the same height, from the same host.
aRestoredSiteReadsTheSame :: Door -> Ground -> IO (Either String ())
aRestoredSiteReadsTheSame door ground = do
  stage <- talk door ground Alice Bob (fresh "carried-across")
  mapM_ (say door (talkReader stage) (talkChannel stage)) ["first", "second", "third"]
  before <- hear door (talkWriter stage) (talkChannel stage)
  sealed <- Service.exporting door (talkWriter stage)
  case sealed of
    Left complaint -> pure (Left ("export was refused: " <> show complaint))
    Right (key, archive) -> do
      let restored = siteOf ground Mallory </> "restored"
      imported <- Door.ask door restored (Door.Import key archive)
      case imported of
        Accepted Imported {} -> do
          after <- hear door restored (talkChannel stage)
          pure $
            if after == before
              then Right ()
              else Left ("the restored site read differently:\n  before: " <> show before <> "\n  after:  " <> show after)
        other -> pure (Left ("the archive did not restore: " <> show other))

-- | Forgetting a channel removes it from the site and nothing from the host.
forgettingLeavesNothingBehind :: Door -> Ground -> IO (Either String ())
forgettingLeavesNothingBehind door ground = do
  kept <- talk door ground Alice Bob (fresh "the-one-that-stays")
  gone <- talk door ground Alice Mallory (fresh "the-one-that-goes")
  mapM_ (\stage -> say door (talkReader stage) (talkChannel stage) "hello" >> hear door (talkWriter stage) (talkChannel stage)) [kept, gone]
  filesBefore <- siteBytes (talkWriter kept)
  objectsBefore <- stored ground
  forgotten <- Door.ask door (talkWriter kept) (Door.Forget (talkChannel gone))
  filesAfter <- siteBytes (talkWriter kept)
  objectsAfter <- stored ground
  reading <- hear door (talkWriter kept) (talkChannel gone)
  listing <- Door.ask door (talkWriter kept) Door.Channels
  pure $ do
    case forgotten of
      Accepted Forgotten {} -> Right ()
      other -> Left ("forget answered " <> show other)
    if length filesAfter < length filesBefore
      then Right ()
      else Left ("forgetting a channel left the site with " <> show (length filesAfter) <> " files, from " <> show (length filesBefore))
    if map fst objectsAfter == map fst objectsBefore
      then Right ()
      else Left "forgetting a channel changed what the host holds"
    case reading of
      Refused complaint -> coded "reading a forgotten channel" complaint
      Accepted outcome -> Left ("a forgotten channel still reads: " <> show outcome)
    case listing of
      Accepted (Channels summaries) | talkChannel gone `notElem` map summaryName summaries -> Right ()
      other -> Left ("a forgotten channel is still listed: " <> show other)

-- | After any sequence of verbs, nothing is left half-written anywhere.
nothingIsLeftHalfWritten :: Door -> Ground -> IO (Either String ())
nothingIsLeftHalfWritten door ground = do
  stage <- talk door ground Alice Bob (fresh "finished-cleanly")
  mapM_ (say door (talkWriter stage) (talkChannel stage)) ["one", "two"]
  _ <- say door (talkReader stage) (talkChannel stage) "three"
  mapM_ (\site -> hear door site (talkChannel stage)) [talkWriter stage, talkReader stage]
  _ <- Door.ask door (talkWriter stage) (Door.Send (fresh "no-such-channel") "refused")
  names <- concat <$> mapM siteNames [talkWriter stage, talkReader stage]
  let staging = waypoint ground </> ".staging"
  createDirectoryIfMissing True staging
  leftovers <- listDirectory staging
  let temporary = [name | name <- names, any (`isSuffixOf` name) [".tmp", "~", ".part"] || "." `isPrefixOf` name]
  pure $ case (temporary, leftovers) of
    ([], []) -> Right ()
    (name : _, _) -> Left ("a site holds a temporary file: " <> name)
    (_, name : _) -> Left ("the host's staging area still holds " <> name)

-- | A verb that is refused has not touched the disk.
aRefusedVerbChangesNothingOnDisk :: Door -> Ground -> IO (Either String ())
aRefusedVerbChangesNothingOnDisk door ground = do
  stage <- talk door ground Alice Bob (fresh "left-exactly-as-found")
  _ <- say door (talkReader stage) (talkChannel stage) "hello"
  _ <- hear door (talkWriter stage) (talkChannel stage)
  let alice = talkWriter stage
  before <- siteBytes alice
  findings <- forM (refusals stage) $ \(what, verb) -> do
    answer <- Door.ask door alice verb
    after <- siteBytes alice
    pure $ case answer of
      Accepted outcome -> Left (what <> " was accepted: " <> show outcome)
      Refused complaint
        | after /= before -> Left (what <> " was refused and still changed the site")
        | otherwise -> coded what complaint
  pure (sequence_ findings)
  where
    refusals stage =
      [ ("accepting your own invitation", Door.Join (talkInvitation stage) (fresh "own-invitation"))
      , ("sending on a channel that is not here", Door.Send (fresh "not-here") "x")
      , ("reading a channel that is not here", Door.Read (fresh "not-here"))
      , ("forgetting a channel that is not here", Door.Forget (fresh "not-here"))
      , ("revoking on a channel that is not here", Door.Revoke (fresh "not-here"))
      , ("importing into an occupied root", Door.Import "0000000000000000000000000000000000000000000000000000000000000000" "x")
      , ("a group naming a channel that is not here", Door.Group (fresh "team") [fresh "not-here"])
      ]

-- | Flip one byte in any one file of a site: the identity reported is the same
-- or the verb is refused with a code; a read is the same answer or a coded
-- refusal. Never a new identity, never a different history, never a crash.
aCorruptedFileNeverChangesWhoYouAre :: Door -> Ground -> IO (Either String ())
aCorruptedFileNeverChangesWhoYouAre door ground = do
  stage <- talk door ground Alice Bob (fresh "trusts-its-disk-not-at-all")
  mapM_ (say door (talkReader stage) (talkChannel stage)) ["alpha", "beta"]
  let alice = talkWriter stage
  identity <- Door.ask door alice Door.Identity
  reading <- hear door alice (talkChannel stage)
  files <- siteBytes alice
  findings <- forM files $ \(path, original) -> do
    ByteString.writeFile path (flipAt (ByteString.length original `div` 2) original)
    who <- Door.ask door alice Door.Identity
    what <- hear door alice (talkChannel stage)
    ByteString.writeFile path original
    pure $ do
      case who of
        Accepted (Identity handle) | Accepted (Identity handle) == identity -> Right ()
        Accepted other -> Left ("with " <> takeFileName path <> " damaged, id became " <> show other)
        Refused complaint -> coded ("id with " <> takeFileName path <> " damaged") complaint
      case what of
        Refused complaint -> coded ("read with " <> takeFileName path <> " damaged") complaint
        answered
          | answered == reading -> Right ()
          | otherwise -> Left ("with " <> takeFileName path <> " damaged, read answered differently: " <> show answered)
  pure (sequence_ findings)

-- | A refusal that carries a stable code, which is the shape every refusal has.
coded :: String -> Complaint -> Either String ()
coded what complaint =
  case complaintCode complaint of
    Code code | Text.null code -> Left (what <> " was refused without a code")
    _ -> Right ()

flipAt :: Int -> ByteString -> ByteString
flipAt index bytes =
  case ByteString.splitAt index bytes of
    (before, rest) | Just (byte, after) <- ByteString.uncons rest -> before <> ByteString.cons (byte + 1) after
    _ -> bytes
