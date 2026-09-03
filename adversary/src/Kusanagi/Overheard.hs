-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What the account at the next desk reads while a command runs.
--
-- The adversary of this module is not the host and not the network. It is
-- another account on the same machine, which on Linux reads any process's
-- arguments out of @\/proc@ and afterwards reads the shell history the first
-- account left behind. It costs nothing and needs no privileges.
--
-- @ARCHITECTURE.md@ §8 ruled the invitation off the command line for exactly
-- that reason. A channel name is worse: an invitation leaks one chance to enter
-- one channel, while @send --to bob@ leaks who is talking to whom on every
-- message — the relationship graph that derived addresses exist to hide.
--
-- So this asserts the shape of the command line itself. It is a pure property
-- and takes microseconds, and it holds the door every other property in this
-- suite now walks through: "Kusanagi.Door" drives every verb with the name on
-- stdin, so a regression here fails seventeen tests rather than one.
module Kusanagi.Overheard
  ( nothingIdentifyingReachesTheCommandLine
  ) where

import Data.ByteString qualified as ByteString
import Data.Maybe (fromMaybe)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import Test.QuickCheck
  ( Gen
  , Property
  , conjoin
  , counterexample
  , elements
  , forAll
  , listOf
  , suchThat
  , vectorOf
  , (===)
  )

import Kusanagi.Answer (ChannelName (..), Invitation (..))
import Kusanagi.Door (Verb (..))
import Kusanagi.Door qualified as Door

-- | Every verb that names a channel, and the one that also carries a message.
speaking :: String -> String -> [Verb]
speaking name text =
  [ Invite channel "/tmp/host" Door.Forever Door.both
  , Join (Invitation "kusanagi1:00") channel
  , Send channel (Text.pack text)
  , Read channel
  , ReadAfter channel 3
  , Revoke channel
  ]
  where
    channel = ChannelName (Text.pack name)

-- | Every argv token that is the program's own word rather than the caller's.
--
-- Taken from the door rather than written down, so it cannot drift: the name
-- fed in here cannot appear in a command line, so whatever comes back is the
-- fixed vocabulary — the verbs, the flags, and the numbers beside them.
--
-- A generated name is kept clear of this set. That is not a weakening: a channel
-- called @read@ would collide with the verb @read@ and prove nothing either way,
-- and the question is whether the caller's word appears, not whether two
-- vocabularies can share a string.
fixed :: [String]
fixed = concatMap Door.spoken (speaking "\0" "\0")

-- | No argument is the caller's name or the caller's message, and the name is
-- still delivered — on stdin, which only this process and its parent can read.
nothingIdentifyingReachesTheCommandLine :: Property
nothingIdentifyingReachesTheCommandLine =
  forAll aName $ \name ->
    forAll aMessage $ \text ->
      conjoin [said name text verb | verb <- speaking name text]
  where
    said name text verb =
      counterexample ("verb: " <> show verb) $
        conjoin
          [ counterexample "the name is on the command line" $
              (name `elem` Door.spoken verb) === False
          , counterexample "the message is on the command line" $
              (text `elem` Door.spoken verb) === False
          , counterexample "the name was not delivered at all" $
              Text.isInfixOf (Text.pack name) (fed verb) === True
          ]
    fed = Text.decodeUtf8Lenient . fromMaybe ByteString.empty . Door.piped

-- | A name the product accepts: 1 to 32 of @a-z0-9-@, never starting with @-@.
aName :: Gen String
aName = ((:) <$> leading <*> fmap (take 31) (listOf plain)) `suchThat` (`notElem` fixed)
  where
    leading = elements (['a' .. 'z'] <> ['0' .. '9'])
    plain = elements (['a' .. 'z'] <> ['0' .. '9'] <> "-")

-- | Something somebody would actually send, and would not want overheard.
aMessage :: Gen String
aMessage = vectorOf 12 (elements (['a' .. 'z'] <> " .")) `suchThat` (`notElem` fixed)
