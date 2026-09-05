-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | The two verbs whose shape is not one question and one answer.
--
-- `ask` is every other verb; these two are the exceptions, and exceptions
-- belong apart rather than in the middle of the rule. An export's result is a
-- file rather than a report, and a host's result is a socket rather than a
-- value. Each reads both streams for what each one is.
module Kusanagi.Service
  ( exporting
  , hosting
  ) where

import Data.ByteString (ByteString)
import Data.Text (Text)
import System.Exit (ExitCode (..))
import System.IO (hGetLine)
import System.Process
  ( CreateProcess (..)
  , StdStream (..)
  , proc
  , withCreateProcess
  )

import Kusanagi.Answer
  ( Complaint
  , Outcome (Exported)
  , decodeComplaint
  , decodeOutcome
  )
import Kusanagi.Door (Door (..))
import Kusanagi.Door qualified as Ask

-- | Seals a site into an archive, and says the key that opens it once.
--
-- The one verb whose result is a file rather than a report: the archive goes
-- to stdout as bytes, the recovery key to stderr as JSON, and nothing keeps a
-- copy of that key. So this cannot go through 'ask', which reads stdout as an
-- outcome; it reads both streams for what each one is.
exporting :: Door -> FilePath -> IO (Either Complaint (Text, ByteString))
exporting door site = do
  said <- Ask.typed door ["--root", site, "--json", "export"] Nothing
  case Ask.typedStatus said of
    ExitSuccess -> case decodeOutcome (Ask.typedErr said) of
      Right (Exported key) -> pure (Right (key, Ask.typedOut said))
      Right other -> fail ("export reported something other than a key: " <> show other)
      Left reason -> fail ("export said its key in a shape this adversary cannot read: " <> reason)
    ExitFailure _ -> either (fail . ("export refused unreadably: " <>)) (pure . Left) (decodeComplaint (Ask.typedErr said))

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
