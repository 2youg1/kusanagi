-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | Somebody with no address, talking raw HTTP to a real host.
--
-- The scanner in `ARCHITECTURE.md` §3 holds no address and is building a
-- list. What it gets from a box is one answer, whatever it asks, and the
-- properties here ask everything a scanner would: the root, the usual
-- well-known paths, malformed addresses, methods the box does not implement,
-- and objects of the wrong size or at paths that climb out of the directory.
-- None of it is allowed to change the host's disk, and all of it must be
-- answered with the same bytes.
module Kusanagi.Scanner
  ( everyStrangerGetsTheSameAnswer
  , aWrongSizeIsNeverStored
  , anAddressIsWrittenOnce
  , traversalTouchesNothing
  ) where

import Control.Exception (bracket)
import Control.Monad (forM)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.ByteString.Char8 qualified as Char8
import Data.Char (toLower)
import Data.List (nub)
import Network.Socket
  ( AddrInfo (..)
  , SocketType (Stream)
  , close
  , connect
  , defaultHints
  , getAddrInfo
  , openSocket
  , withSocketsDo
  )
import Network.Socket.ByteString (recv, sendAll)
import System.Directory (doesDirectoryExist, listDirectory)
import System.FilePath (takeDirectory)
import System.Timeout (timeout)

import Kusanagi.Door (Door)
import Kusanagi.Service qualified as Service
import Kusanagi.Ground (Ground, waypoint)
import Kusanagi.Stage (siteBytes)

-- | Every question a scanner asks gets one answer, byte for byte.
everyStrangerGetsTheSameAnswer :: Door -> Ground -> IO (Either String ())
everyStrangerGetsTheSameAnswer door ground =
  Service.hosting door (waypoint ground) $ \host -> do
    answers <- forM strangers $ \(what, request) -> (,) what <$> exchange host request
    let distinct = nub (map snd answers)
        telling = [what | (what, answer) <- answers, "kusanagi" `Char8.isInfixOf` Char8.map toLower answer || "server:" `Char8.isInfixOf` Char8.map toLower answer]
    pure $ do
      case distinct of
        [_] -> Right ()
        _ -> Left ("a scanner gets " <> show (length distinct) <> " different answers:\n" <> unlines [what <> ": " <> show a | (what, a) <- answers])
      case telling of
        [] -> Right ()
        (what : _) -> Left ("the answer to " <> what <> " names a server or the project")
  where
    strangers =
      [ ("the root", get "/")
      , ("robots.txt", get "/robots.txt")
      , ("a well-known path", get "/.well-known/security.txt")
      , ("the drop prefix alone", get "/d/")
      , ("a short address", get "/d/zz")
      , ("an uppercase address", get ("/d/" <> Char8.map toUpperHex address))
      , ("a 41-digit address", get ("/d/" <> address <> "0"))
      , ("an address nobody wrote", get ("/d/" <> address))
      , ("OPTIONS", "OPTIONS / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
      , ("POST to a drop", "POST /d/" <> address <> " HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
      , ("DELETE of a drop", "DELETE /d/" <> address <> " HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
      , ("HEAD of the root", "HEAD / HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
      ]
    toUpperHex c = if c >= 'a' && c <= 'f' then toEnum (fromEnum c - 32) else c

-- | Objects of any size but the one size are never stored.
aWrongSizeIsNeverStored :: Door -> Ground -> IO (Either String ())
aWrongSizeIsNeverStored door ground =
  Service.hosting door (waypoint ground) $ \host -> do
    before <- objects ground
    _ <- forM [0, 1, 131071, 131073, 200000] $ \size ->
      exchange host (put ("/d/" <> address) (ByteString.replicate size 0x41))
    after <- objects ground
    back <- exchange host (get ("/d/" <> address))
    pure $ do
      if after == before then Right () else Left ("a wrong-sized object was stored: " <> show (length after - length before) <> " new file(s)")
      if "200" `ByteString.isInfixOf` ByteString.take 20 back then Left "a wrong-sized object reads back" else Right ()

-- | The first write at an address stands; the second changes nothing.
anAddressIsWrittenOnce :: Door -> Ground -> IO (Either String ())
anAddressIsWrittenOnce door ground =
  Service.hosting door (waypoint ground) $ \host -> do
    let first = ByteString.replicate 131072 0x41
        second = ByteString.replicate 131072 0x42
    one <- exchange host (put ("/d/" <> address) first)
    two <- exchange host (put ("/d/" <> address) second)
    back <- exchange host (get ("/d/" <> address))
    pure $
      if first `ByteString.isSuffixOf` back
        then Right ()
        else Left ("the second write at an address won, or the first was never stored: " <> show (ByteString.take 60 back) <> "\n  first put: " <> show (ByteString.take 60 one) <> "\n  second put: " <> show (ByteString.take 60 two))

-- | Paths that climb out of the directory reach nothing and create nothing.
traversalTouchesNothing :: Door -> Ground -> IO (Either String ())
traversalTouchesNothing door ground =
  Service.hosting door (waypoint ground) $ \host -> do
    let parent = takeDirectory (waypoint ground)
    outsideBefore <- listDirectory parent
    before <- objects ground
    answers <- forM climbing $ \path -> exchange host (put path (ByteString.replicate 131072 0x43))
    fetched <- forM climbing $ \path -> exchange host (get path)
    outsideAfter <- listDirectory parent
    after <- objects ground
    pure $ do
      if outsideAfter == outsideBefore then Right () else Left ("a traversal created something outside the host: " <> show outsideAfter)
      if after == before then Right () else Left "a traversal created an object inside the host"
      case [a | a <- answers <> fetched, "200" `ByteString.isInfixOf` ByteString.take 20 a] of
        [] -> Right ()
        (a : _) -> Left ("a traversal was answered 200: " <> show (ByteString.take 80 a))
  where
    climbing =
      [ "/d/../../escaped"
      , "/d/..%2f..%2fescaped"
      , "/d/%2e%2e/%2e%2e/escaped"
      , "/../escaped"
      , "/d/" <> ByteString.take 38 address <> "/.."
      , "/d/" <> address <> "%00"
      ]

-- | A key as a host files one: a period, a ward and an address.
address :: ByteString
address = "0000000000000001/00ab/0123456789abcdef0123456789abcdef01234567"

get :: ByteString -> ByteString
get path = "GET " <> path <> " HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n"

put :: ByteString -> ByteString -> ByteString
put path body =
  "PUT " <> path <> " HTTP/1.1\r\nHost: x\r\nIf-None-Match: *\r\nContent-Length: "
    <> Char8.pack (show (ByteString.length body))
    <> "\r\nConnection: close\r\n\r\n"
    <> body

-- | Every file the host directory holds, by path.
objects :: Ground -> IO [FilePath]
objects ground = do
  there <- doesDirectoryExist (waypoint ground)
  if there then map fst <$> siteBytes (waypoint ground) else pure []

-- | One request, one connection, the whole answer.
exchange :: String -> ByteString -> IO ByteString
exchange host request = withSocketsDo $ do
  let (name, rest) = break (== ':') host
  found <- getAddrInfo (Just defaultHints {addrSocketType = Stream}) (Just name) (Just (drop 1 rest))
  case found of
    [] -> fail ("nothing resolves " <> host)
    (info : _) -> bracket (openSocket info) close $ \socket -> do
      connect socket (addrAddress info)
      sendAll socket request
      answer <- timeout 10_000_000 (drain socket ByteString.empty)
      pure (maybe "no answer within ten seconds" id answer)
  where
    drain socket acc = do
      chunk <- recv socket 65_536
      if ByteString.null chunk then pure acc else drain socket (acc <> chunk)
