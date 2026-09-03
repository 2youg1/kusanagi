-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | The person standing in front of the host, who sees packets and not bytes.
--
-- Every other property here takes the host's position: it holds the objects, so
-- it can weigh them, count them and compare them. This module takes the position
-- of whoever carries the traffic — a network, a proxy, an employer, an internet
-- provider. That observer never sees a plaintext and never sees a key. It sees
-- **when a packet went past, and which way**.
--
-- __The host itself must not be the one keeping this record.__
-- @ARCHITECTURE.md@ §3 line 0 says a host learns nothing, so a host that wrote
-- down request times would be a host that had learned something. The record is
-- kept out here instead, by a relay that forwards every byte unchanged and adds
-- one line to a list on the way past. The product is not modified, not
-- configured and not aware of it, which is the only way this measurement is
-- evidence about the product rather than about a debug mode.
module Kusanagi.Relay
  ( Observation (..)
  , Relay
  , withRelay
  , locator
  , observed
  ) where

import Control.Concurrent (forkIO, killThread)
import Control.Concurrent.MVar (newEmptyMVar, takeMVar, tryPutMVar)
import Control.Exception (SomeException, bracket, bracketOnError, finally, try)
import Control.Monad (forever, void)
import Data.ByteString qualified as ByteString
import Data.ByteString.Char8 qualified as Char8
import Data.IORef (IORef, atomicModifyIORef', newIORef, readIORef)
import Data.Text (Text)
import Data.Text.Encoding qualified as Text
import GHC.Clock (getMonotonicTime)
import Network.Socket
  ( AddrInfo (..)
  , AddrInfoFlag (AI_PASSIVE)
  , Socket
  , SocketOption (ReuseAddr)
  , SocketType (Stream)
  , accept
  , bind
  , close
  , connect
  , defaultHints
  , getAddrInfo
  , listen
  , openSocket
  , setSocketOption
  , shutdown
  , socketPort
  , withSocketsDo
  )
import Network.Socket (ShutdownCmd (ShutdownSend))
import Network.Socket.ByteString (recv, sendAll)
import System.Timeout (timeout)

import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door

-- | One request going past, as much of it as a carrier can see.
--
-- The time is monotonic, so that a clock correction in the middle of a run
-- cannot produce a negative interval and a feature built on nonsense.
data Observation = Observation
  { observedAt :: Double
  , observedMethod :: Text
  , observedPath :: Text
  }
  deriving stock (Eq, Show)

-- | A host, and the wire everything to it goes down.
data Relay = Relay
  { relayLocator :: String
  , relaySeen :: IORef [Observation]
  }

-- | Where an endpoint is pointed so that its traffic passes this observer.
locator :: Relay -> String
locator = relayLocator

-- | Everything that went past, oldest first.
observed :: Relay -> IO [Observation]
observed = fmap reverse . readIORef . relaySeen

-- | Runs a real host behind a relay for the duration of an action.
--
-- The directory is the one the host serves, which is the same directory
-- "Kusanagi.Ground" reads afterwards: what a host holds does not depend on how
-- an endpoint reached it, so one world answers both kinds of question.
withRelay :: Door -> FilePath -> (Relay -> IO a) -> IO a
withRelay door directory act =
  withSocketsDo . Door.hosting door directory $ \upstream -> do
    forwarding <- resolve upstream
    bracket listening close $ \gate -> do
      seen <- newIORef []
      port <- socketPort gate
      accepting <- forkIO (forever (handOff forwarding seen gate))
      let relay = Relay {relayLocator = "http://127.0.0.1:" <> show port, relaySeen = seen}
      -- **The socket closes before the thread is killed, and the order is the
      -- whole of it.** `accept` is a blocking foreign call; a Haskell thread
      -- sitting inside one cannot take an asynchronous exception, so with the
      -- threaded runtime `killThread` waits for a call that will never return on
      -- its own. Closing the socket makes that call return, and only then is
      -- there a thread there to kill. `close` twice is harmless, which is why
      -- the enclosing bracket can still do its job.
      act relay `finally` (swallow (close gate) >> killThread accepting)

-- | A socket listening on a port nobody chose.
listening :: IO Socket
listening = do
  let hints = defaultHints {addrFlags = [AI_PASSIVE], addrSocketType = Stream}
  found <- getAddrInfo (Just hints) (Just "127.0.0.1") (Just "0")
  case found of
    [] -> fail "no local address to listen on"
    -- `bracketOnError`, not `bracket`: the socket is the value being returned,
    -- so it must survive success and must not survive a half-finished setup.
    (address : _) ->
      bracketOnError (openSocket address) close $ \gate -> do
        setSocketOption gate ReuseAddr 1
        bind gate (addrAddress address)
        listen gate 64
        pure gate

-- | Where the real host is.
resolve :: String -> IO AddrInfo
resolve upstream = do
  let (host, rest) = break (== ':') upstream
      port = drop 1 rest
  found <- getAddrInfo (Just defaultHints {addrSocketType = Stream}) (Just host) (Just port)
  case found of
    [] -> fail ("the host announced an address nothing resolves: " <> upstream)
    (address : _) -> pure address

-- | Accepts one connection and carries it, without holding up the next one.
handOff :: AddrInfo -> IORef [Observation] -> Socket -> IO ()
handOff forwarding seen gate = do
  (client, _) <- accept gate
  void . forkIO $ swallow (carry forwarding seen client) `finally` close client

-- | Everything one connection carries, and the one line it leaves behind.
--
-- The box closes the connection after answering, so one connection is one
-- request and the head arrives before anything else. Recording it here rather
-- than counting bytes is deliberate: a carrier reading a TLS stream would see
-- neither method nor path, and the point of writing them down is to have a
-- counterexample somebody can read, not to claim the carrier has them.
carry :: AddrInfo -> IORef [Observation] -> Socket -> IO ()
carry forwarding seen client =
  bracket (openSocket forwarding) close $ \server -> do
    connect server (addrAddress forwarding)
    request <- head' ByteString.empty
    at <- getMonotonicTime
    note seen at request
    sendAll server request
    -- The half-close dance, and it is not ceremony. `sendAll` returns when the
    -- operating system has taken the bytes, not when the far end has read them,
    -- so a relay that closed as soon as one direction ended would throw away a
    -- response it had already accepted — which an endpoint reports as
    -- `waypoint.io`, a fault in this instrument dressed as a fault in the host.
    asked <- newEmptyMVar
    _ <- forkIO $
      (swallow (pour client server) >> swallow (shutdown server ShutdownSend))
        `finally` void (tryPutMVar asked ())
    swallow (pour server client)
    swallow (shutdown client ShutdownSend)
    -- Bounded, because a client that never closes must not hold a test open.
    void (timeout 5_000_000 (takeMVar asked))
  where
    -- Bounded, because a request head that never ends is a request this relay
    -- refuses to buffer rather than one it waits out.
    head' acc
      | "\r\n\r\n" `ByteString.isInfixOf` acc = pure acc
      | ByteString.length acc > 16_384 = pure acc
      | otherwise = do
          chunk <- recv client 4_096
          if ByteString.null chunk then pure acc else head' (acc <> chunk)

-- | Copies one direction until it ends.
pour :: Socket -> Socket -> IO ()
pour from to = do
  chunk <- recv from 65_536
  if ByteString.null chunk
    then pure ()
    else sendAll to chunk >> pour from to

-- | Records the first line, and nothing else.
note :: IORef [Observation] -> Double -> ByteString.ByteString -> IO ()
note seen at request =
  case Char8.words (Char8.takeWhile (/= '\r') request) of
    (method : path : _) ->
      atomicModifyIORef' seen $ \earlier ->
        ( Observation
            { observedAt = at
            , observedMethod = Text.decodeUtf8Lenient method
            , observedPath = Text.decodeUtf8Lenient path
            }
            : earlier
        , ()
        )
    _ -> pure ()

-- | A socket that is being torn down from the other side is not a finding.
--
-- Both ends of a relayed connection close, and whichever thread loses that race
-- gets an exception from a handle the other one has already closed. That is the
-- normal end of a connection rather than a fault, and letting it escape would
-- print a stack trace into a test run that had gone perfectly.
swallow :: IO () -> IO ()
swallow act = void (try act :: IO (Either SomeException ()))
