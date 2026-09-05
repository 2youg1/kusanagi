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
  , binOf
  , corrupt
  , damage
  , holding
  , plant
  , vanish
  , transplant
  ) where

import Control.Monad (forM)
import Data.ByteString qualified as ByteString
import Data.List (sort)
import Data.Text (Text)
import Data.Text qualified as Text
import System.Directory
  ( createDirectoryIfMissing
  , doesDirectoryExist
  , listDirectory
  , removeFile
  )
import System.FilePath (joinPath, takeDirectory, takeFileName, (</>))
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
    shard prefix (Address rest, bytes) = (Address (Text.pack prefix <> "/" <> rest), bytes)

-- | Where a host keeps one object.
--
-- An address here is the whole key a host files a drop under,
-- @period/ward/address@, which is what a send reports and what a directory
-- host uses as a path. That is the one implementation detail this adversary
-- depends on; when it changes, these properties fail loudly rather than
-- silently testing nothing.
placed :: Ground -> Address -> FilePath
placed ground (Address key) =
  groundHost ground </> joinPath (map Text.unpack (Text.splitOn "/" key))

-- | The bin a key sits in: everything before the address.
binOf :: Address -> Text
binOf (Address key) = Text.intercalate "/" (init' (Text.splitOn "/" key))
  where
    init' xs = take (length xs - 1) xs

-- | Flips one bit of an object, the way damage or a hostile host would.
corrupt :: Ground -> Address -> IO ()
corrupt ground address = do
  let path = placed ground address
  bytes <- ByteString.readFile path
  case ByteString.uncons bytes of
    Nothing -> fail ("the host is holding nothing at " <> show address)
    Just (first, rest) -> ByteString.writeFile path (ByteString.cons (first + 1) rest)

-- | Changes the byte at one offset, which is how damage and a hostile host
-- both look from the reader's side. Offsets past the end change nothing.
damage :: Ground -> Int -> Address -> IO ()
damage ground offset address = do
  let path = placed ground address
  bytes <- ByteString.readFile path
  case ByteString.splitAt offset bytes of
    (before, rest) | Just (byte, after) <- ByteString.uncons rest ->
      ByteString.writeFile path (before <> ByteString.cons (byte + 1) after)
    _ -> pure ()

-- | The bytes the host holds at one address.
holding :: Ground -> Address -> IO ByteString.ByteString
holding ground = ByteString.readFile . placed ground

-- | Puts whatever bytes the host likes at an address, whether or not anything
-- was there. A host that can write its own disk can do this; the question is
-- only ever what a reader makes of it.
plant :: Ground -> Address -> ByteString.ByteString -> IO ()
plant ground address bytes = do
  let path = placed ground address
  createDirectoryIfMissing True (takeDirectory path)
  ByteString.writeFile path bytes

-- | Drops an object the host was holding.
--
-- A host cannot forge a segment, but it can always refuse to hand one over, and
-- refusing selectively is a lie about history rather than an outage. Nothing
-- stops it; what it must not achieve is a reader believing less than that reader
-- has already verified.
vanish :: Ground -> Address -> IO ()
vanish ground = removeFile . placed ground

-- | Serves the object from one address at another.
--
-- The strongest move a store gets for free. It forges nothing and corrupts
-- nothing: every byte it hands over is a byte an endpoint really wrote and
-- really signed. What it changes is only *where* those bytes are, and this
-- network answers that with the key rather than with a check — an address
-- derives the key its contents are sealed under, so bytes that arrive at the
-- wrong address do not open at all.
transplant :: Ground -> Address -> Address -> IO ()
transplant ground from to = do
  bytes <- ByteString.readFile (placed ground from)
  ByteString.writeFile (placed ground to) bytes
