-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}

-- | The only place that knows a binary exists.
--
-- Everything this adversary learns, it learns by running the program a user runs,
-- with the arguments a user types, and reading the two streams a user reads.
-- There is no linking, no FFI, and no shared type: what cannot be reached
-- through this module cannot be tested here, which is the point.
module Kusanagi.Door
  ( Door
  , Verb (..)
  , Abilities (..)
  , Lifetime (..)
  , Typed (..)
  , both
  , sendOnly
  , readOnly
  , neither
  , seconds
  , discover
  , ask
  , typed
  , spoken
  ) where

import Data.ByteString qualified as ByteString
import Data.List (intercalate)
import Data.Text (Text)
import Data.Text qualified as Text
import System.Directory (doesFileExist)
import System.Environment (lookupEnv)
import System.Exit (ExitCode (..))
import System.FilePath ((</>))
import System.IO (hClose, hSetBinaryMode)
import System.Process
  ( CreateProcess (..)
  , StdStream (..)
  , proc
  , waitForProcess
  , withCreateProcess
  )

-- Only the names this module needs: `Outcome` carries a `Read` constructor, and
-- so does `Verb` below. Importing the whole of it would make both ambiguous.
import Kusanagi.Answer
  ( Answer (..)
  , ChannelName (..)
  , Invitation (..)
  , decodeComplaint
  , decodeOutcome
  )

-- | A binary that has already been built.
newtype Door = Door FilePath
  deriving stock (Eq, Show)

-- | What an invitation lets its holder do.
--
-- Two independent flags rather than an ordered level, because the product has
-- two independent abilities and an order would invent a rule nobody wrote.
data Abilities = Abilities
  { maySend :: Bool
  , mayRead :: Bool
  }
  deriving stock (Eq, Ord, Show)

both, sendOnly, readOnly, neither :: Abilities
both = Abilities True True
sendOnly = Abilities True False
readOnly = Abilities False True
neither = Abilities False False

-- | How long an invitation stands.
--
-- Two values, not a number: the adversary never predicts a clock, so the only
-- distinction it can honestly draw is between an invitation that has already
-- expired and one that will not expire during a test.
data Lifetime = Forever | Instantly
  deriving stock (Eq, Ord, Show)

seconds :: Lifetime -> Word
seconds Forever = 3600
seconds Instantly = 0

-- | One thing to ask the program to do.
data Verb
  = Identity
  | Channels
  | Invite ChannelName FilePath Lifetime Abilities
  | Join Invitation ChannelName
  | Send ChannelName Text
  | Read ChannelName
  | Revoke ChannelName
  deriving stock (Eq, Show)

-- | Finds the binary, preferring what the caller was told to use.
--
-- The justfile builds it and passes the path in @KUSANAGI_BIN@, so there is one
-- authority for where it is; the fallbacks exist only so that a person poking at
-- @cabal repl@ is not stopped by an environment variable.
discover :: IO (Maybe Door)
discover =
  lookupEnv "KUSANAGI_BIN" >>= \case
    Just path -> pick [path]
    Nothing -> pick (concatMap flavours ["debug", "release"])
  where
    flavours profile =
      [ ".." </> "target" </> profile </> "kusanagi"
      , ".." </> "target" </> profile </> "kusanagi.exe"
      ]
    pick [] = pure Nothing
    pick (candidate : rest) = do
      there <- doesFileExist candidate
      if there then pure (Just (Door candidate)) else pick rest

-- | Asks one question of one endpoint, and reads the answer.
--
-- A refusal is an answer. A stream that will not parse is not: it means the
-- door has changed shape, so this throws rather than reporting a green test
-- against a program it can no longer read.
ask :: Door -> FilePath -> Verb -> IO Answer
ask (Door binary) site verb = do
  (status, out, err) <- capture binary (argv site verb) Nothing
  case status of
    ExitSuccess -> either (unreadable out) (pure . Accepted) (decodeOutcome out)
    ExitFailure _ -> either (unreadable err) (pure . Refused) (decodeComplaint err)
  where
    unreadable raw reason =
      fail $
        "the door answered in a shape this adversary cannot read: "
          <> reason
          <> "\n  asked: "
          <> show verb
          <> "\n  said:  "
          <> show (ByteString.take 400 raw)

-- | What a command line did, before anything decides whether that was allowed.
--
-- `ask` turns this into an `Answer` and throws when it cannot. Typing badly on
-- purpose needs the layer underneath: an exit code the door is not supposed to
-- produce is exactly what a keyboard test is looking for, and it must be able to
-- see one rather than crash on it.
data Typed = Typed
  { typedStatus :: ExitCode
  , typedOut :: ByteString.ByteString
  , typedErr :: ByteString.ByteString
  }
  deriving stock (Eq, Show)

-- | Runs the binary with exactly these arguments, and these bytes on stdin.
--
-- No `--root`, no `--json`, nothing added: what is passed here is what a person
-- typed or an agent spawned, character for character.
typed :: Door -> [String] -> Maybe ByteString.ByteString -> IO Typed
typed (Door binary) arguments input = do
  (status, out, err) <- capture binary arguments input
  pure (Typed status out err)

argv :: FilePath -> Verb -> [String]
argv site verb = ["--root", site, "--json"] <> spoken verb

spoken :: Verb -> [String]
spoken = \case
  Identity -> ["id"]
  Channels -> ["channels"]
  Invite (ChannelName name) waypoint lifetime abilities ->
    [ "invite"
    , "--name"
    , Text.unpack name
    , "--waypoint"
    , waypoint
    , "--for"
    , show (seconds lifetime)
    , "--can"
    , listed abilities
    ]
  Join (Invitation line) (ChannelName name) ->
    ["join", Text.unpack line, "--name", Text.unpack name]
  Send (ChannelName name) text ->
    ["send", "--to", Text.unpack name, Text.unpack text]
  Read (ChannelName name) -> ["read", "--from", Text.unpack name]
  Revoke (ChannelName name) -> ["revoke", "--from", Text.unpack name]

listed :: Abilities -> String
listed abilities =
  intercalate
    ","
    ([word | (word, held) <- [("send", maySend abilities), ("read", mayRead abilities)], held])

-- | Runs the program and takes both streams as bytes.
--
-- Bytes rather than text: the recovery lines carry punctuation outside ASCII,
-- and a locale-decoded stream would corrupt them on the way in and then fail to
-- parse for a reason that has nothing to do with the product.
capture ::
  FilePath ->
  [String] ->
  Maybe ByteString.ByteString ->
  IO (ExitCode, ByteString.ByteString, ByteString.ByteString)
capture binary arguments input =
  withCreateProcess
    (proc binary arguments)
      { std_in = maybe NoStream (const CreatePipe) input
      , std_out = CreatePipe
      , std_err = CreatePipe
      }
    $ \stdin out err handle ->
      case (out, err) of
        (Just outHandle, Just errHandle) -> do
          hSetBinaryMode outHandle True
          hSetBinaryMode errHandle True
          case (stdin, input) of
            (Just inHandle, Just payload) -> do
              hSetBinaryMode inHandle True
              ByteString.hPut inHandle payload
              hClose inHandle
            _ -> pure ()
          reported <- ByteString.hGetContents outHandle
          complained <- ByteString.hGetContents errHandle
          status <- waitForProcess handle
          pure (status, reported, complained)
        _ -> fail "the child process was created without the pipes it was asked for"
