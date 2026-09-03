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
  , hosting
  , typed
  , typedWith
  , spoken
  , piped
  ) where

import Control.Exception (IOException, try)
import Control.Monad (void)
import Data.ByteString qualified as ByteString
import Data.List (intercalate)
import Data.Text (Text)
import Data.Text.Encoding qualified as Text
import Data.Word (Word64)
import System.Directory (doesFileExist)
import System.Environment (lookupEnv)
import System.Exit (ExitCode (..))
import System.FilePath ((</>))
import System.IO (hClose, hGetLine, hSetBinaryMode)
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
  | ReadAfter ChannelName Word64
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
  (status, out, err) <- capture binary (argv site verb) (piped verb)
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

-- | Runs a host for the duration of an action, and hands over the address it took.
--
-- The one verb that never returns needs a shape of its own: the result of
-- @kusanagi host@ is not a value on stdout but a socket somebody else can
-- connect to, and that address is announced on stderr as the last word of the
-- first line. **The address is asked for rather than chosen** (@--bind 0@),
-- because a test that picks a port has already lost a race with every other
-- test on the machine.
--
-- Reading that line is also the readiness signal: it is written once the
-- listener is up, so an action that begins by connecting will find something
-- there.
hosting :: Door -> FilePath -> (String -> IO a) -> IO a
hosting (Door binary) directory act =
  withCreateProcess
    (proc binary ["host", "--dir", directory, "--bind", "0"])
      { std_in = NoStream
      , std_out = CreatePipe
      , std_err = CreatePipe
      }
    $ \_ _ err _ ->
      case err of
        Nothing -> fail "the host was created without the pipe it was asked for"
        Just errHandle -> do
          announced <- hGetLine errHandle
          case reverse (words announced) of
            (address : _) -> act address
            [] -> fail ("the host announced no address: " <> show announced)

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
typed door arguments input = typedWith door Nothing arguments input

-- | The same, with the whole environment chosen rather than inherited.
--
-- Some of what this program does is decided by the environment rather than by
-- an argument: where a site goes when nobody says, and whether a proxy stands in
-- front of every request. A test that cannot set the environment cannot ask
-- about either, and the alternative — changing this process's own environment —
-- would leak into every other property running beside it.
--
-- @Nothing@ inherits. @Just pairs@ replaces: a variable that is not in the list
-- is not in the child's environment at all, which is how "this machine will not
-- say where data lives" is asked.
typedWith ::
  Door ->
  Maybe [(String, String)] ->
  [String] ->
  Maybe ByteString.ByteString ->
  IO Typed
typedWith (Door binary) surroundings arguments input = do
  (status, out, err) <- captureIn surroundings binary arguments input
  pure (Typed status out err)

argv :: FilePath -> Verb -> [String]
argv site verb = ["--root", site, "--json"] <> spoken verb

-- | What a verb needs on stdin, which is everything that identifies anybody.
--
-- A command line is public: every account on the machine reads another
-- process's arguments while it runs, and the shell keeps them afterwards. The
-- product answered that for the invitation first, and then for its own kind — a
-- channel name leaks who is talking to whom on every single message, which is
-- the relationship graph the derived addresses exist to hide.
--
-- So @-@ stands in for every name here, and the first line of stdin carries it.
-- **Every property in this suite runs through that path**, which is what makes
-- a regression in it fail a test rather than pass unnoticed.
piped :: Verb -> Maybe ByteString.ByteString
piped = \case
  Identity -> Nothing
  Channels -> Nothing
  Invite name _ _ _ -> Just (line name)
  Join (Invitation invitation) name -> Just (line name <> Text.encodeUtf8 invitation)
  Send name text -> Just (line name <> Text.encodeUtf8 text)
  Read name -> Just (line name)
  ReadAfter name _ -> Just (line name)
  Revoke name -> Just (line name)
  where
    line (ChannelName name) = Text.encodeUtf8 (name <> "\n")

-- | The command line, which now names nobody and quotes nothing.
spoken :: Verb -> [String]
spoken = \case
  Identity -> ["id"]
  Channels -> ["channels"]
  Invite _ waypoint lifetime abilities ->
    [ "invite"
    , "--name"
    , onStdin
    , "--waypoint"
    , waypoint
    , "--for"
    , show (seconds lifetime)
    , "--can"
    , listed abilities
    ]
  Join _ _ -> ["join", "--name", onStdin]
  Send _ _ -> ["send", "--to", onStdin]
  Read _ -> ["read", "--from", onStdin]
  ReadAfter _ floor' -> ["read", "--from", onStdin, "--after", show floor']
  Revoke _ -> ["revoke", "--from", onStdin]

-- | What a name argument says when the name itself arrives on stdin.
onStdin :: String
onStdin = "-"

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
capture = captureIn Nothing

captureIn ::
  Maybe [(String, String)] ->
  FilePath ->
  [String] ->
  Maybe ByteString.ByteString ->
  IO (ExitCode, ByteString.ByteString, ByteString.ByteString)
captureIn surroundings binary arguments input =
  withCreateProcess
    (proc binary arguments)
      { std_in = maybe NoStream (const CreatePipe) input
      , std_out = CreatePipe
      , std_err = CreatePipe
      , env = surroundings
      }
    $ \stdin out err handle ->
      case (out, err) of
        (Just outHandle, Just errHandle) -> do
          hSetBinaryMode outHandle True
          hSetBinaryMode errHandle True
          case (stdin, input) of
            -- A broken pipe here is the product working. Every verb reads a
            -- bounded amount of stdin and then stops, so feeding it more than
            -- the bound closes the far end mid-write — and a harness that could
            -- not survive that could not ask about the bound at all.
            (Just inHandle, Just payload) -> do
              hSetBinaryMode inHandle True
              void (try (ByteString.hPut inHandle payload) :: IO (Either IOException ()))
              hClose inHandle
            _ -> pure ()
          reported <- ByteString.hGetContents outHandle
          complained <- ByteString.hGetContents errHandle
          status <- waitForProcess handle
          pure (status, reported, complained)
        _ -> fail "the child process was created without the pipes it was asked for"
