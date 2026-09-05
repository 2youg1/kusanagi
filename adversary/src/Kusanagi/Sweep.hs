-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What a host's access log says about a read, now that a read names a bin.
--
-- The relay in front of a real host sees every request line. Before D-20 a
-- reader asked @GET /address@ and the log paired the writer of that address
-- with its reader. Now a reader asks @GET /bin/period/ward/@ and then fetches
-- every key the answer listed — strangers' objects included — so the log
-- holds a ward being read and never which object in it was wanted.
--
-- Two properties, both relations between the log and the disk rather than an
-- expected trace: **a read fetches exactly the bin**, and **no request of a
-- read or a send names an address outside a bin the same command listed**.
module Kusanagi.Sweep
  ( aReadFetchesTheWholeBinAndNothingElse
  , noRequestNamesAnUnlistedAddress
  ) where

import Control.Monad (forM_)
import Data.ByteString qualified as ByteString
import Data.List (sort)
import Data.Text (Text)
import Data.Text qualified as Text

import Kusanagi.Answer (Address (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..), binOf, plant, stored)
import Kusanagi.Ground qualified as Ground
import Kusanagi.Relay (Observation (..), observed, withRelay)
import Kusanagi.Relay qualified as Relay
import Kusanagi.Stage

-- | The request paths the relay saw after a point, with the method that made them.
since :: Int -> [Observation] -> [(Text, Text)]
since skipped = map (\o -> (observedMethod o, observedPath o)) . drop skipped

-- | Every key on the host that sits in @bin@.
inBin :: Ground -> Text -> IO [Text]
inBin ground bin = do
  held <- stored ground
  pure (sort [key | (Address key, _) <- held, binOf (Address key) == bin])

-- | Alice writes three drops to Bob; the host adds two objects of its own to
-- Bob's bin. Bob's first read fetches every object in the bin — his three and
-- the two strangers — and reports exactly his three. A reader that fetched a
-- subset would be telling the host which subset was its own.
aReadFetchesTheWholeBinAndNothingElse :: Door -> Ground -> IO (Either String ())
aReadFetchesTheWholeBinAndNothingElse door ground =
  withRelay door (Ground.waypoint ground) $ \relay -> do
    stage <- talkWith door ground Alice Bob (Door.Invite (fresh "taken-whole") (Relay.locator relay) Door.Forever Door.both)
    said <- mapM (say door (talkWriter stage) (talkChannel stage)) ["one", "two", "three"]
    bin <- case said of
      (first : _) -> pure (binOf first)
      [] -> fail "nothing was said"
    forM_ [0 :: Int, 1] $ \n ->
      plant ground (Address (bin <> "/" <> Text.replicate 40 (Text.singleton (if n == 0 then 'a' else 'b')))) (ByteString.replicate 131072 (fromIntegral n))
    before <- length <$> observed relay
    heard <- hear door (talkReader stage) (talkChannel stage)
    requests <- since before <$> observed relay
    everything <- inBin ground bin
    let fetched = sort [Text.drop 3 path | ("GET", path) <- requests, "/d/" `Text.isPrefixOf` path]
        listings = [path | ("GET", path) <- requests, "/bin/" `Text.isPrefixOf` path]
    pure $ do
      if null listings then Left "the read listed no bin" else Right ()
      if fetched == everything
        then Right ()
        else Left ("the read fetched " <> show fetched <> " and the bin holds " <> show everything)
      case entriesOf heard of
        Right three | length three == 3 -> Right ()
        other -> Left ("a bin with two strangers in it changed the read: " <> show other)

-- | Across an invitation, three sends and two reads, every request that names
-- an object names one under a bin the same side listed first, and every
-- listing names a period and a ward and nothing more. The rendezvous — the
-- offer and the greeting, in period zero — is the one exception, fetched by
-- address once and written down as such.
noRequestNamesAnUnlistedAddress :: Door -> Ground -> IO (Either String ())
noRequestNamesAnUnlistedAddress door ground =
  withRelay door (Ground.waypoint ground) $ \relay -> do
    stage <- talkWith door ground Alice Bob (Door.Invite (fresh "never-an-address") (Relay.locator relay) Door.Forever Door.both)
    afterJoin <- length <$> observed relay
    _ <- mapM (say door (talkWriter stage) (talkChannel stage)) ["one", "two", "three"]
    _ <- hear door (talkReader stage) (talkChannel stage)
    _ <- say door (talkReader stage) (talkChannel stage) "four"
    _ <- hear door (talkWriter stage) (talkChannel stage)
    requests <- since afterJoin <$> observed relay
    let listed = [Text.drop 5 path | ("GET", path) <- requests, "/bin/" `Text.isPrefixOf` path]
        -- Period zero is the rendezvous: the offer and the greeting sit there,
        -- fetched by address once, and `seal::rendezvous` says why.
        named = [Text.drop 3 path | (_, path) <- requests, "/d/" `Text.isPrefixOf` path, not ("/d/0000000000000000/" `Text.isPrefixOf` path)]
        unlisted = [key | key <- named, not (any (`Text.isPrefixOf` key) listed)]
        malformed = [prefix | prefix <- listed, length (Text.splitOn "/" (Text.dropWhileEnd (== '/') prefix)) /= 2]
    pure $ do
      if null listed then Left "nothing was listed" else Right ()
      case unlisted of
        [] -> Right ()
        (path : _) -> Left ("a request named " <> Text.unpack path <> ", which no listing on this side preceded")
      case malformed of
        [] -> Right ()
        (prefix : _) -> Left ("a listing asked for " <> Text.unpack prefix <> ", which is not a period and a ward")
      case [() | ("DELETE", _) <- requests] of
        [] -> Right ()
        _ -> Left "a delete named an address"
