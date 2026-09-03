-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | The edges of the command line itself, asked from outside the program.
--
-- "Kusanagi.Keyboard" asks what happens when somebody types the wrong thing.
-- This asks about the four places where the shape of the door is decided by
-- something other than the characters typed: what arrives on the pipe, what
-- clap does with a line it cannot parse, what an argument the verb cannot act
-- on produces, and where a site lands when nobody says.
--
-- __These claims used to be Rust integration tests, and that was the wrong
-- room.__ A Rust test links the library, so nothing stops one of these from
-- reaching past the command line into a function \u2014 and the day it does, it is
-- still called a test of the door. Out here there is no library to reach: one
-- subprocess, two streams, one exit code. `just boxes` is the gate that keeps it
-- that way.
module Kusanagi.Doorway
  ( invitationSurvivesAnyClipboard
  , anEmptyPipeIsAnswered
  , aFloodOnStdinIsAnswered
  , anArgumentTheVerbCannotActOnIsRefused
  , aMistypedFlagLeavesByTheSameDoor
  , aSiteNobodyPlacedLandsUnderTheProfile
  , aMachineThatWillNotSayIsAsked
  ) where

import Data.ByteString qualified as ByteString
import Data.ByteString.Char8 qualified as Char8
import Data.List (isInfixOf)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Directory (doesDirectoryExist, listDirectory)
import System.Environment (getEnvironment)
import System.Exit (ExitCode (..))
import System.FilePath ((</>))
import System.Info (os)

import Kusanagi.Answer
  ( Answer (..)
  , ChannelName (..)
  , Code (..)
  , Complaint (..)
  , Invitation (..)
  , Outcome (..)
  , decodeComplaint
  )
import Kusanagi.Door (Door, Typed (..))
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, siteOf, waypoint)
import Kusanagi.Ground qualified as Ground

-- | Every way a clipboard mangles a line, and none of them may lose it.
--
-- The invitation stopped being an argument because an argument is public. That
-- moved it onto a pipe and created a new set of edges instead: a paste with a
-- trailing newline, one with none, one from a Windows clipboard carrying
-- @\\r\\n@, and one a chat window padded with spaces and blank lines.
--
-- One invitation admits exactly one endpoint, so each clipboard gets its own.
invitationSurvivesAnyClipboard :: Door -> Ground -> IO (Either String ())
invitationSurvivesAnyClipboard door ground = go (zip [0 :: Int ..] clipboards)
  where
    clipboards =
      [ id
      , (<> "\n")
      , (<> "\r\n")
      , \line -> "  " <> line <> "  \n\n"
      ]
    go [] = pure (Right ())
    go ((round', paste) : rest) = do
      let channel = ChannelName (Text.pack ("bob-" <> show round'))
          inviter = siteOf ground Ground.Alice
          joiner = waypoint ground <> "-joiner-" <> show round'
      minted <-
        Door.ask door inviter (Door.Invite channel (waypoint ground) Door.Forever Door.both)
      case minted of
        Accepted (Invited _ (Invitation line) _) -> do
          joined <-
            Door.ask door joiner (Door.Join (Invitation (paste line)) (ChannelName "alice"))
          case joined of
            Accepted Joined {} -> go rest
            other ->
              pure
                ( Left
                    ( "clipboard " <> show round' <> " did not join: " <> show other )
                )
        other -> pure (Left ("the invitation was refused: " <> show other))

-- | Somebody typed the command and forgot the pipe.
--
-- The answer has to be the ordinary shape \u2014 a stable code and a way out \u2014 rather
-- than a wait. And the way out has to name the pipe, because there is no other
-- way in: advice that says \"copy the invitation\" without saying where to put it
-- sends a person looking for a flag this program does not have.
anEmptyPipeIsAnswered :: Door -> Ground -> IO (Either String ())
anEmptyPipeIsAnswered door ground = do
  said <- refusal door ground ["join", "--name", "alice"] ""
  pure $ do
    complaint <- said
    if complaintCode complaint /= Code "kusanagi.malformed"
      then Left ("an empty pipe was answered with " <> show (complaintCode complaint))
      else
        let way = Text.unpack (complaintRecover complaint)
         in if "pipe" `isInfixOf` way && "join" `isInfixOf` way
              then Right ()
              else Left ("the way out of an empty pipe does not mention it: " <> way)

-- | Far more than an invitation can be.
--
-- The bound inside the program decides this rather than the parser, and what
-- matters is that it ends at all: a door that buffers whatever arrives has
-- handed the caller a way to spend this process's memory.
aFloodOnStdinIsAnswered :: Door -> Ground -> IO (Either String ())
aFloodOnStdinIsAnswered door ground = do
  said <- refusal door ground ["join", "--name", "alice"] (Char8.replicate 1_000_000 'x')
  pure $ do
    complaint <- said
    let Code code = complaintCode complaint
    if Text.null code then Left "a flood on stdin was answered with no code" else Right ()

-- | An ability nobody defined, offered to a verb that takes a list of them.
--
-- Refused rather than ignored, and the way out has to name what to pass instead:
-- an invitation that silently granted less than it was asked for is discovered
-- by the person it was given to, days later, as a failure they cannot explain.
anArgumentTheVerbCannotActOnIsRefused :: Door -> Ground -> IO (Either String ())
anArgumentTheVerbCannotActOnIsRefused door ground = do
  said <-
    refusal
      door
      ground
      ["invite", "--name", "bob", "--waypoint", waypoint ground, "--can", "send,fly"]
      ""
  pure $ do
    complaint <- said
    if complaintCode complaint /= Code "kusanagi.argument"
      then Left ("an unknown ability was answered with " <> show (complaintCode complaint))
      else
        if "send" `isInfixOf` Text.unpack (complaintRecover complaint)
          then Right ()
          else Left "the recovery does not say what to pass instead"

-- | One missed key on a flag, which is not this program's error path at all.
--
-- Found by this adversary once already: @-root@ for @--root@ reached clap's own
-- reporting, which exits with a code this door does not define and prints prose
-- even when the caller asked for JSON. An agent cannot act on that. Help is the
-- other half of the same claim \u2014 it is what a person asks for, so it succeeds and
-- goes to stdout.
aMistypedFlagLeavesByTheSameDoor :: Door -> Ground -> IO (Either String ())
aMistypedFlagLeavesByTheSameDoor door ground = do
  refused <- Door.typed door ["--json", "-root", siteOf ground Ground.Alice, "id"] Nothing
  asked <- Door.typed door ["--help"] Nothing
  pure $ do
    ensure (typedStatus refused == ExitFailure 1) ("a mistyped flag exited " <> show (typedStatus refused))
    ensure (ByteString.null (typedOut refused)) "a refusal put something on stdout"
    complaint <-
      either
        (\reason -> Left ("a refusal a program cannot parse is not an answer: " <> reason))
        Right
        (decodeComplaint (typedErr refused))
    ensure
      (complaintCode complaint == Code "kusanagi.argument")
      ("a mistyped flag was answered with " <> show (complaintCode complaint))
    ensure (not (Text.null (complaintRecover complaint))) "the refusal carries no way out"
    ensure (typedStatus asked == ExitSuccess) "--help failed"
    ensure
      ("forget" `isInfixOf` Text.unpack (Text.decodeUtf8Lenient (typedOut asked)))
      "--help does not list the verbs"

-- | Where a site lands when nobody says where.
--
-- A relative default put an identity, every channel key and every cairn in
-- whatever directory the program happened to be started from \u2014 for an agent, the
-- repository it is editing or a folder a sync client uploads. Nothing about that
-- is visible at the moment it happens.
--
-- Windows only, and that is the honest limit: the program compiles one branch
-- per platform and this machine can only run one of them.
aSiteNobodyPlacedLandsUnderTheProfile :: Door -> Ground -> IO (Either String ())
aSiteNobodyPlacedLandsUnderTheProfile door ground
  | os /= "mingw32" = pure (Right ())
  | otherwise = do
      let profile = waypoint ground <> "-profile"
      surroundings <- withProfile (Just profile)
      answered <- Door.typedWith door (Just surroundings) ["--json", "id"] Nothing
      landed <- doesDirectoryExist (profile </> "kusanagi")
      here <- listDirectory "."
      pure $ do
        ensure (typedStatus answered == ExitSuccess) "`id` failed with a profile set"
        ensure landed "no site appeared under the profile directory"
        ensure
          (".kusanagi" `notElem` here)
          "a site appeared in the current directory, which is what the default was moved away from"

-- | A machine that will not say where data lives is asked rather than guessed.
aMachineThatWillNotSayIsAsked :: Door -> Ground -> IO (Either String ())
aMachineThatWillNotSayIsAsked door _ground
  | os /= "mingw32" = pure (Right ())
  | otherwise = do
      surroundings <- withProfile Nothing
      answered <- Door.typedWith door (Just surroundings) ["--json", "id"] Nothing
      pure $ do
        ensure (typedStatus answered /= ExitSuccess) "a site was placed with nothing to place it by"
        complaint <-
          either (\reason -> Left ("the refusal did not parse: " <> reason)) Right
            (decodeComplaint (typedErr answered))
        ensure
          (complaintCode complaint == Code "kusanagi.no_root")
          ("the refusal was " <> show (complaintCode complaint))
        ensure
          ("--root" `isInfixOf` Text.unpack (complaintRecover complaint))
          "the way out does not name --root"

-- | This process's environment with @LOCALAPPDATA@ set to something, or removed.
withProfile :: Maybe FilePath -> IO [(String, String)]
withProfile chosen = do
  inherited <- getEnvironment
  let without = [pair | pair@(name, _) <- inherited, name /= "LOCALAPPDATA"]
  pure (maybe without (\path -> ("LOCALAPPDATA", path) : without) chosen)

-- | Runs one command that is expected to be refused, and hands back the refusal.
refusal ::
  Door ->
  Ground ->
  [String] ->
  ByteString.ByteString ->
  IO (Either String Complaint)
refusal door ground arguments fed = do
  answered <-
    Door.typed door (["--root", siteOf ground Ground.Bob, "--json"] <> arguments) (Just fed)
  pure $
    if typedStatus answered == ExitSuccess
      then Left ("this was supposed to be refused: " <> show arguments)
      else
        either
          (\reason -> Left ("a refusal a program cannot parse is not an answer: " <> reason))
          Right
          (decodeComplaint (typedErr answered))

-- | A condition, with the sentence to print when it does not hold.
ensure :: Bool -> String -> Either String ()
ensure True _ = Right ()
ensure False reason = Left reason
