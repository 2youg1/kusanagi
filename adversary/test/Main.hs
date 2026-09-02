-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | Four groups, in the order that loses the least time when something breaks.
--
-- The renderer is checked first because it takes milliseconds and because a
-- broken deliverable makes every counterexample below it worthless. Random
-- traces come next, then the directed attack, then the host's simplest lie.
--
-- With no binary to drive, this exits successfully and says so. A gate that
-- could not run is not a gate that failed, and treating it as one is how a
-- Haskell toolchain would end up blocking Rust contributors.
module Main (main) where

import Control.Monad.Reader (ReaderT, asks, liftIO, runReaderT)
import Data.List (sort)
import Data.Text qualified as Text
import Data.Text.IO qualified as Text
import System.Directory (doesFileExist)
import System.Environment (lookupEnv)
import System.FilePath ((</>))
import Data.ByteString qualified as ByteString
import Data.Word (Word8)
import Test.QuickCheck (Property, counterexample, ioProperty, property, withNumTests)
import Test.QuickCheck.DynamicLogic (forAllDL)
import Test.QuickCheck.Monadic (PropertyM, monadic, run)
import Test.QuickCheck.StateModel (Actions, Any (..), mkVar, runActions)
import Test.Tasty (TestTree, defaultMain, testGroup)
import Test.Tasty.HUnit (assertBool, assertEqual, testCase)
import Test.Tasty.QuickCheck (testProperty)

import Kusanagi.Answer (Address (..))
import Kusanagi.Answer qualified as Answer
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground
import Kusanagi.Keyboard
  ( Bench (..)
  , Choice
  , Typing (..)
  , adviceIsAboutWhatWasGiven
  , adviceIsExecutable
  , bytesSurviveTheTrip
  , prepare
  , shapeIsAnswerable
  , typingOf
  )
import Kusanagi.Model
import Kusanagi.Regression (coherent, render, sequenced)

main :: IO ()
main = do
  found <- Door.discover
  case found of
    Nothing ->
      putStrLn "skipped: no kusanagi binary to drive. Run `just adversary`, or set KUSANAGI_BIN."
    Just door -> defaultMain (properties door)

properties :: Door -> TestTree
properties door =
  testGroup
    "adversary"
    [ testCase "the committed Rust test is what this adversary renders" deliverable
    , testProperty "a mistyped line is answerable, and its advice can be taken" (keyboard door)
    , testProperty "what an agent pipes in comes back byte for byte" (piping door)
    , testProperty "what one endpoint says is what the other hears" (traces door)
    , testProperty "a revoked peer is never readable again" (revocation door)
    , testProperty "a corrupted object is refused, not believed" (tampering door)
    ]

-- | Somebody types the line from the README with one finger in the wrong place.
--
-- Three things are asserted about whatever comes back, and none of them is an
-- expected output: it has one of the two shapes this door defines, every command
-- its advice names can be taken, and the advice is about something that was
-- actually supplied.
keyboard :: Door -> Choice -> Property
keyboard door choice = withNumTests 24 . ioProperty . withGround $ \ground -> do
  bench <- benchOn door ground
  let typing = typingOf bench choice
  answered <- shapeIsAnswerable door (keyed typing)
  case answered of
    Left reason -> pure (counterexample (typed typing reason) False)
    Right (Answer.Accepted _) -> pure (property True)
    Right (Answer.Refused complaint) -> do
      executable <- adviceIsExecutable door (benchSite bench) complaint
      let about = adviceIsAboutWhatWasGiven (keyed typing) complaint
      pure $ case (executable, about) of
        (Left reason, _) -> counterexample (typed typing reason) False
        (_, Left reason) -> counterexample (typed typing reason) False
        _ -> property True
  where
    typed typing reason =
      reason
        <> "\n  meant:  kusanagi "
        <> unwords (intended typing)
        <> "\n  typed:  kusanagi "
        <> unwords (keyed typing)
        <> "\n  slip:   "
        <> show (slipped typing)

-- | An agent sends bytes it did not choose the shape of.
piping :: Door -> [Word8] -> Property
piping door bytes = withNumTests 12 . ioProperty . withGround $ \ground -> do
  bench <- benchOn door ground
  outcome <- bytesSurviveTheTrip door (benchSite bench) (benchChannel bench) (ByteString.pack bytes)
  pure $ case outcome of
    Left reason -> counterexample reason False
    Right () -> property True

benchOn :: Door -> Ground -> IO Bench
benchOn door ground =
  prepare door (siteOf ground Alice) (siteOf ground Bob) (waypoint ground)

-- | Rule four, made mechanical.
--
-- The renderer is the authority for that Rust file, and the file is the proof
-- that what the renderer emits compiles and passes. Neither can move without
-- the other, and neither is a place where a rule could be restated.
deliverable :: IO ()
deliverable = do
  assertBool "the committed trace is not one the model admits" (coherent remembered)
  accepting <- lookupEnv "KUSANAGI_ACCEPT"
  if accepting == Just "1"
    then Text.writeFile path rendered
    else do
      there <- doesFileExist path
      assertBool ("there is no " <> path <> "; rerun with KUSANAGI_ACCEPT=1") there
      onDisk <- Text.readFile path
      assertEqual "the committed Rust test is not what this adversary renders" rendered onDisk
  where
    rendered = render "an_endpoint_cannot_accept_its_own_invitation" remembered
    path = ".." </> "crates" </> "kusanagi" </> "tests" </> "from_adversary.rs"

-- | The counterexample this adversary found, as it was minimised.
--
-- Before the fix, the third and fourth steps passed: accepting your own
-- invitation gave one endpoint two local names for one stream — both derived
-- from the same secret and the same author — so a read handed back what that
-- endpoint had itself just written, as though a peer had said it. An agent
-- reading its own output as input is a feedback loop, not a conversation.
--
-- `join` now refuses at the first step of that, which is why the second action
-- below renders as a refusal.
remembered :: Actions World
remembered =
  sequenced
    [ Some (Invite Alice one Door.Forever Door.both)
    , Some (Join Alice (mkVar 1) two)
    , Some (Send Alice one "beta")
    , Some (Read Alice one)
    ]
  where
    one = Answer.ChannelName "one"
    two = Answer.ChannelName "two"

-- | Any trace at all, plus what the host is left holding afterwards.
traces :: Door -> Actions World -> Property
traces door actions =
  withNumTests 20 . driving door $ do
    _ <- runActions actions
    held <- run (asks kitGround >>= liftIO . stored)
    pure (unlinkable (map fst held))

-- | Any prefix, one revocation, any suffix, and a read that must still fail.
revocation :: Door -> Property
revocation door = withNumTests 10 (forAllDL revocationIsFinal (traces door))

-- | The simplest lie a host can tell: one changed byte.
--
-- What the reader must never do is show it. Nothing in the command asks for a
-- check, which is the point — verification is not an option a caller can forget.
tampering :: Door -> Property
tampering door = withNumTests 1 . ioProperty . withGround $ \ground -> do
  let one = Answer.ChannelName "one"
      at site = Door.ask door (siteOf ground site)
  invitation <-
    at Alice (Door.Invite one (waypoint ground) Door.Forever Door.both) >>= \answer ->
      case answer of
        Answer.Accepted (Answer.Invited _ line _) -> pure line
        other -> fail ("the invitation was refused: " <> show other)
  joined <- at Bob (Door.Join invitation one)
  address <-
    at Bob (Door.Send one "a message that must arrive intact") >>= \answer ->
      case (joined, answer) of
        (Answer.Accepted Answer.Joined {}, Answer.Accepted (Answer.Sent _ _ left)) -> pure left
        other -> fail ("nothing was written to corrupt: " <> show other)
  before <- at Alice (Door.Read one)
  case before of
    Answer.Accepted (Answer.Read _ _ _ [_]) -> do
      corrupt ground address
      after <- at Alice (Door.Read one)
      pure (refused after)
    other -> fail ("the segment did not arrive intact: " <> show other)

-- | Runs a trace against the real program in a world that is thrown away.
driving :: Door -> PropertyM (ReaderT Kit IO) Bool -> Property
driving door act =
  monadic (\attempt -> ioProperty (withGround (runReaderT attempt . Kit door))) act

-- | Whether the host's view links anything to anything.
--
-- Two properties in one: an address is never reused, and no two of them look
-- alike. A host that could group drops by a shared prefix would have the
-- relationship graph this design exists to withhold.
unlinkable :: [Address] -> Bool
unlinkable addresses = all apart (zip sorted (drop 1 sorted))
  where
    -- Sorted, so the pair with the longest shared prefix is always adjacent.
    sorted = sort addresses
    apart (Address earlier, Address later) =
      earlier /= later && maybe True (\(shared, _, _) -> Text.length shared < 8) (Text.commonPrefixes earlier later)

refused :: Answer.Answer -> Bool
refused (Answer.Refused _) = True
refused _ = False
