-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | A world that exists for one test, and the host's power to lie in it.
--
-- The host is the untrusted half of this network, so reaching into its
-- directory is not going behind the product's back — it is taking the position
-- the threat model already grants an adversary. Everything an endpoint does
-- still goes through "Kusanagi.Door".
module Kusanagi.Ground
  ( Ground
  , Site (..)
  , cast
  , named
  , withGround
  , siteOf
  , waypoint
  , stored
  , corrupt
  ) where

import Control.Monad (forM)
import Data.ByteString qualified as ByteString
import Data.List (sort)
import Data.Text qualified as Text
import System.Directory (createDirectoryIfMissing, doesDirectoryExist, listDirectory)
import System.FilePath (takeFileName, (</>))
import System.IO.Temp (withSystemTempDirectory)

import Kusanagi.Answer (Address (..))

-- | The cast. Three endpoints are enough for every question a two-party channel
-- can raise: two to talk, and one to try what it was never invited to.
data Site = Alice | Bob | Mallory
  deriving stock (Eq, Ord, Show, Enum, Bounded)

cast :: [Site]
cast = [minBound .. maxBound]

named :: Site -> FilePath
named Alice = "alice"
named Bob = "bob"
named Mallory = "mallory"

-- | One throwaway world: a host nobody trusts, and the sites that use it.
data Ground = Ground
  { groundRoot :: FilePath
  , groundHost :: FilePath
  }
  deriving stock (Eq, Show)

-- | Runs an action in a world that is deleted afterwards.
withGround :: (Ground -> IO a) -> IO a
withGround act =
  withSystemTempDirectory "kusanagi-adversary" $ \root -> do
    let ground = Ground {groundRoot = root, groundHost = root </> "host"}
    createDirectoryIfMissing True (groundHost ground)
    act ground

-- | Where one endpoint keeps its identity and channels.
siteOf :: Ground -> Site -> FilePath
siteOf ground site = groundRoot ground </> named site

-- | The host, as a locator an endpoint can be pointed at.
waypoint :: Ground -> FilePath
waypoint = groundHost

-- | Everything the host holds, which is everything the host knows.
--
-- Sorted, so that a property about the host's view does not accidentally depend
-- on the order a directory happened to be walked in.
stored :: Ground -> IO [(Address, ByteString.ByteString)]
stored ground = sort <$> walk (groundHost ground)
  where
    walk directory = do
      entries <- listDirectory directory
      fmap concat . forM entries $ \entry ->
        if entry == ".staging"
          then pure []
          else do
            let path = directory </> entry
            isDirectory <- doesDirectoryExist path
            if isDirectory
              then map (shard entry) <$> walk path
              else do
                bytes <- ByteString.readFile path
                pure [(Address (Text.pack (takeFileName path)), bytes)]
    shard prefix (Address rest, bytes) = (Address (Text.pack prefix <> rest), bytes)

-- | Where a host keeps one object.
--
-- The first two characters of an address name a directory. That is the one
-- implementation detail this adversary depends on; when it changes, these
-- properties fail loudly rather than silently testing nothing.
placed :: Ground -> Address -> FilePath
placed ground (Address address) =
  groundHost ground </> Text.unpack shard </> Text.unpack rest
  where
    (shard, rest) = Text.splitAt 2 address

-- | Flips one bit of an object, the way damage or a hostile host would.
corrupt :: Ground -> Address -> IO ()
corrupt ground address = do
  let path = placed ground address
  bytes <- ByteString.readFile path
  case ByteString.uncons bytes of
    Nothing -> fail ("the host is holding nothing at " <> show address)
    Just (first, rest) -> ByteString.writeFile path (ByteString.cons (first + 1) rest)
