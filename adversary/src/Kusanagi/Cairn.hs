-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | What a reader must still be told after it has started remembering.
--
-- An endpoint now writes down how far it has verified a stream, so that the next
-- read resumes instead of naming every address of the stream to the host again.
-- That is a privacy fix, and the way a privacy fix of this shape goes wrong is
-- by quietly reading less than it reports: a resumed walk that started too high
-- would hand back a short list and a correct height, and every assertion about
-- one message arriving would still pass.
--
-- So both properties here are relations between two traces of the same run, and
-- neither names an expected output:
--
-- * a read that names a floor agrees, entry for entry, with a read that names
--   none — with exactly the entries at or below the floor missing;
-- * reading twice reports what reading once reported, which is the assertion
--   that the memory written by the first read cannot subtract from the second.
--
-- Neither reaches into a site. Whether the memory is a file, where it lives, and
-- what it is called are all things this module must not know, because the day it
-- knows them is the day it stops testing the door and starts testing the
-- implementation.
module Kusanagi.Cairn
  ( Stocked (..)
  , stock
  , floorHidesExactlyWhatItNames
  , readingTwiceSubtractsNothing
  ) where

import Data.List (sortOn)
import Data.Text qualified as Text
import Data.Word (Word64)

import Kusanagi.Answer
  ( Answer (..)
  , ChannelName (..)
  , Entry (..)
  , Outcome (..)
  )
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door

-- | A channel with a known number of segments on the peer's stream.
data Stocked = Stocked
  { stockedReader :: FilePath
  , stockedChannel :: ChannelName
  , stockedHeight :: Word64
  }
  deriving stock (Eq, Show)

-- | Opens a channel and puts @count@ segments on it, from writer to reader.
--
-- Nothing is read here. A reader that has never read has nothing written down,
-- so every property below starts from the state in which the fix is not yet
-- doing anything — and reaches the state in which it is by reading, which is
-- what a caller does too.
stock :: Door -> FilePath -> FilePath -> FilePath -> Int -> IO Stocked
stock door writer reader host count = do
  minted <- Door.ask door writer (Door.Invite channel host Door.Forever Door.both)
  invitation <- case minted of
    Accepted (Invited _ line _) -> pure line
    other -> fail ("the invitation was refused: " <> show other)
  joined <- Door.ask door reader (Door.Join invitation channel)
  case joined of
    Accepted Joined {} -> pure ()
    other -> fail ("the channel could not be joined: " <> show other)
  mapM_ say [1 .. count]
  pure
    Stocked
      { stockedReader = reader
      , stockedChannel = channel
      , stockedHeight = fromIntegral count
      }
  where
    channel = ChannelName "peer"
    say n = do
      said <- Door.ask door writer (Door.Send channel (Text.pack ("segment " <> show n)))
      case said of
        Accepted Sent {} -> pure ()
        other -> fail ("a segment was refused: " <> show other)

-- | A floor hides the entries at or below it, and hides nothing else.
--
-- The height is compared too, and separately. A resumed read that lost the
-- stream's head would be a different bug with the same cause, and reporting the
-- right segments under the wrong height is worse than failing: an agent uses the
-- height to decide where to poll from next.
floorHidesExactlyWhatItNames :: Door -> Stocked -> Word64 -> IO (Either String ())
floorHidesExactlyWhatItNames door stocked level = do
  whole <- readingOf door stocked Nothing
  above <- readingOf door stocked (Just level)
  pure $ case (whole, above) of
    (Left reason, _) -> Left ("reading the whole stream failed: " <> reason)
    (_, Left reason) -> Left ("reading above " <> show level <> " failed: " <> reason)
    (Right (wholeHeight, entries), Right (aboveHeight, found))
      | wholeHeight /= aboveHeight ->
          Left
            ( "--after "
                <> show level
                <> " changed the reported height from "
                <> show wholeHeight
                <> " to "
                <> show aboveHeight
            )
      | found /= expected ->
          Left
            ( "--after "
                <> show level
                <> " reported "
                <> show (map entryIndex found)
                <> " where the whole stream minus that floor is "
                <> show (map entryIndex expected)
            )
      | otherwise -> Right ()
      where
        expected = filter ((> level) . entryIndex) entries

-- | Reading again reports everything reading once reported.
--
-- The first read is what makes an endpoint remember; this asserts the second is
-- not paid for out of what it hands back.
readingTwiceSubtractsNothing :: Door -> Stocked -> Maybe Word64 -> IO (Either String ())
readingTwiceSubtractsNothing door stocked level = do
  first <- readingOf door stocked level
  again <- readingOf door stocked level
  pure $ case (first, again) of
    (Left reason, _) -> Left ("the first read failed: " <> reason)
    (_, Left reason) -> Left ("the second read failed: " <> reason)
    (Right before, Right after)
      | before /= after ->
          Left
            ( "reading twice with --after "
                <> show level
                <> " gave "
                <> show (summarise before)
                <> " then "
                <> show (summarise after)
            )
      | otherwise -> Right ()
  where
    summarise (height, entries) = (height, map entryIndex entries)

-- | One read, reduced to the two facts a caller acts on.
--
-- Entries are put in index order rather than trusted to arrive in it: this
-- module is asserting which entries came back, and letting an ordering bug
-- masquerade as a missing-segment bug would point the next reader at the wrong
-- file.
readingOf ::
  Door -> Stocked -> Maybe Word64 -> IO (Either String (Maybe Word64, [Entry]))
readingOf door stocked level = do
  answer <- Door.ask door (stockedReader stocked) verb
  pure $ case answer of
    Accepted (Read _ _ height entries) -> Right (height, sortOn entryIndex entries)
    other -> Left (show other)
  where
    verb = case level of
      Nothing -> Door.Read (stockedChannel stocked)
      Just floor' -> Door.ReadAfter (stockedChannel stocked) floor'
