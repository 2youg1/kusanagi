-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | Something that answers a socket the way a host should not.
--
-- "Kusanagi.Relay" stands in front of a real host and changes nothing. This
-- stands in the host's place and follows a script: hold the connection open
-- and say nothing, answer with bytes that are not a box's, or send the client
-- somewhere else. What it records is what a network adversary would learn —
-- that a connection arrived, and the request head it carried — so a property
-- can say "this listener was never connected to" and mean it.
module Kusanagi.Listener
  ( Script (..)
  , Listener
  , withListener
  , locatorOf
  , portOf
  , connections
  , heads
  ) where

import Control.Concurrent (forkIO, killThread, threadDelay)
import Control.Concurrent.MVar (MVar, newMVar, modifyMVar_, readMVar)
import Control.Exception (SomeException, bracket, bracketOnError, finally, try)
import Control.Monad (forever, void)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.IORef (IORef, atomicModifyIORef', newIORef, readIORef)
import Network.Socket
  ( AddrInfo (..)
  , AddrInfoFlag (AI_PASSIVE)
  , Socket
  , SocketOption (ReuseAddr)
  , SocketType (Stream)
  , accept
  , bind
  , close
  , defaultHints
  , getAddrInfo
  , listen
  , openSocket
  , setSocketOption
  , socketPort
  , withSocketsDo
  )
import Network.Socket.ByteString (recv, sendAll)

-- | What to do with each connection that arrives.
data Script
  = -- | Accept, read the head, and never answer.
    BlackHole
  | -- | Answer with exactly these bytes, then close.
    Answer ByteString
  | -- | Answer with a 302 to this locator, then close.
    Redirect String
  deriving stock (Eq, Show)

data Listener = Listener
  { listenerPort :: Int
  , listenerConnections :: IORef Int
  , listenerHeads :: IORef [ByteString]
  , listenerHeld :: MVar [Socket]
  }

-- | Where an endpoint is pointed to reach this listener as a host.
locatorOf :: Listener -> String
locatorOf listener = "http://127.0.0.1:" <> show (listenerPort listener) <> "/"

portOf :: Listener -> Int
portOf = listenerPort

-- | How many connections arrived.
connections :: Listener -> IO Int
connections = readIORef . listenerConnections

-- | Every request head that arrived, oldest first.
heads :: Listener -> IO [ByteString]
heads = fmap reverse . readIORef . listenerHeads

-- | Runs a scripted listener for the duration of an action.
withListener :: Script -> (Listener -> IO a) -> IO a
withListener script act =
  withSocketsDo . bracket listening close $ \gate -> do
    port <- socketPort gate
    arrived <- newIORef 0
    seen <- newIORef []
    held <- newMVar []
    let listener =
          Listener
            { listenerPort = fromIntegral port
            , listenerConnections = arrived
            , listenerHeads = seen
            , listenerHeld = held
            }
    -- Swallowed, because closing the gate below makes the blocked `accept`
    -- throw, and that throw is the loop ending rather than a finding.
    accepting <- forkIO (swallow (forever (handOff script listener gate)))
    -- The socket closes before the thread is killed; see "Kusanagi.Relay" for
    -- why that order is the whole of it. Connections a black hole is holding
    -- are closed last, which is what lets a client waiting on one give up.
    act listener
      `finally` (swallow (close gate) >> killThread accepting >> readMVar held >>= mapM_ (swallow . close))

listening :: IO Socket
listening = do
  let hints = defaultHints {addrFlags = [AI_PASSIVE], addrSocketType = Stream}
  found <- getAddrInfo (Just hints) (Just "127.0.0.1") (Just "0")
  case found of
    [] -> fail "no local address to listen on"
    (address : _) ->
      bracketOnError (openSocket address) close $ \gate -> do
        setSocketOption gate ReuseAddr 1
        bind gate (addrAddress address)
        listen gate 64
        pure gate

handOff :: Script -> Listener -> Socket -> IO ()
handOff script listener gate = do
  (client, _) <- accept gate
  atomicModifyIORef' (listenerConnections listener) (\n -> (n + 1, ()))
  void . forkIO $ swallow (follow script listener client)

follow :: Script -> Listener -> Socket -> IO ()
follow script listener client = do
  request <- head' ByteString.empty
  atomicModifyIORef' (listenerHeads listener) (\earlier -> (request : earlier, ()))
  case script of
    BlackHole -> do
      modifyMVar_ (listenerHeld listener) (pure . (client :))
      -- Keep reading so the client's body is drained rather than refused,
      -- which is what a host that is merely slow looks like.
      forever (recv client 65_536 >>= \chunk -> if ByteString.null chunk then threadDelay 3_600_000_000 else pure ())
    Answer bytes -> (sendAll client bytes >> threadDelay 200_000) `finally` close client
    Redirect elsewhere ->
      sendAll
        client
        ( "HTTP/1.1 302 Found\r\nLocation: "
            <> encode elsewhere
            <> "\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        `finally` close client
  where
    head' acc
      | "\r\n\r\n" `ByteString.isInfixOf` acc = pure acc
      | ByteString.length acc > 16_384 = pure acc
      | otherwise = do
          chunk <- recv client 4_096
          if ByteString.null chunk then pure acc else head' (acc <> chunk)
    encode = ByteString.pack . map (fromIntegral . fromEnum)

swallow :: IO () -> IO ()
swallow act = void (try act :: IO (Either SomeException ()))
