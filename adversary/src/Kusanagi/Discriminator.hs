-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | The experiment an adversary holding our source code would run.
--
-- Every other property here asks whether the product obeys a rule. This one asks
-- the question that matters once the repository is public: **given as many
-- labelled examples as they care to generate, can somebody tell the two worlds
-- apart?** They can run the binary too. Whatever residual difference exists,
-- they will find it, and no amount of argument on our side changes that.
--
-- So the argument is replaced by a measurement. Two experiments, both against
-- real worlds built by the real binary:
--
-- * **Volume.** Two worlds with the same number of messages, one carrying a byte
--   each and one carrying three thousand. Nothing may separate them. This is the
--   claim the fixed-size envelope in @kusanagi_seal::veil@ exists to make, and
--   before that envelope every size feature below would have separated them at a
--   glance.
-- * **Presence.** A channel where nothing is said, against one where something
--   is. Here exactly one thing separates them — how many drops there are — and
--   the property asserts it is the /only/ thing. A leak that is measured and
--   written down is a decision; a leak that is argued about is a hope.
--
-- **The assertion is an equality, so it bites in both directions.** A new
-- feature that starts separating turns it red, which is the regression case. A
-- declared leak that stops separating also turns it red, which is the day
-- somebody lands cover traffic and has to come here and say so.
--
-- **Why single-threshold rules rather than a classifier.** A stump that
-- separates the two groups /is/ the rule a censor deploys: one number, one
-- comparison, no model to ship. If no stump separates them the practical attack
-- is closed, and the counterexample this module prints is a sentence a person
-- can read instead of a set of weights.
module Kusanagi.Discriminator
  ( Reading (..)
  , features
  , separating
  , volumeSaysNothing
  , presenceSaysOnlyHowMany
  ) where

import Control.Monad (replicateM)
import Data.ByteString qualified as ByteString
import Data.List (intercalate, sort)
import Data.Set qualified as Set
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Word (Word8)

import Kusanagi.Answer (Address (..), Answer (..), ChannelName (..), Outcome (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, stored, waypoint, withGround)

-- | How many worlds are built for each side of an experiment.
--
-- Four. A threshold that happens to order eight random numbers into two clean
-- groups turns up once in thirty-five tries, which is far too often across ten
-- features — so `separates` also demands a margin, and these two rules together
-- are what keep this from being a test that fails on Tuesdays.
sides :: Int
sides = 4

-- | The features that are allowed to separate a silent channel from a busy one.
--
-- One fact in two units: the host holds one object per thing said, so counting
-- objects counts the conversation. Closing it needs traffic that does not depend
-- on whether anybody is talking, which is not built. Until it is, this list is
-- where that gap is recorded, and the property below fails if the list is wrong
-- in either direction.
declared :: [Text]
declared = ["bytes.total", "drops"]

-- | One number a censor could put a threshold on.
data Reading = Reading
  { readingName :: Text
  , readingValue :: Double
  }
  deriving stock (Eq, Show)

-- | Everything measurable in what a host holds, without holding any key.
--
-- Deliberately wider than what we expect to be closed. A feature nobody thought
-- of is the one that catches the next mistake, and carrying an extra measurement
-- costs one line.
features :: [(Address, ByteString.ByteString)] -> [Reading]
features held =
  [ Reading "drops" (count bodies)
  , Reading "bytes.total" (fromIntegral (sum sizes))
  , Reading "size.smallest" (fromIntegral (smallest sizes))
  , Reading "size.largest" (fromIntegral (largest sizes))
  , Reading "size.distinct" (count (distinct sizes))
  , Reading "name.length" (fromIntegral (largest nameLengths))
  , Reading "byte.mean" (mean everyByte)
  , Reading "byte.distinct" (count (distinct everyByte))
  , Reading "byte.repeats" (repeats bodies)
  , Reading "constant.positions" (fromIntegral (constantPositions bodies))
  ]
  where
    bodies = map snd held
    sizes = map ByteString.length bodies
    everyByte = concatMap ByteString.unpack bodies
    nameLengths = [Text.length name | (Address name, _) <- held]

count :: [a] -> Double
count = fromIntegral . length

distinct :: Ord a => [a] -> [a]
distinct = Set.toList . Set.fromList

smallest :: [Int] -> Int
smallest [] = 0
smallest values = minimum values

largest :: [Int] -> Int
largest [] = 0
largest values = maximum values

mean :: [Word8] -> Double
mean [] = 0
mean values = sum (map fromIntegral values) / count values

-- | How often one byte equals the byte before it.
--
-- The cheapest compressibility test there is, and the one that matters: text,
-- structure and a pad of zeroes all repeat, and a keystream does not. Roughly
-- one in 256 for anything properly encrypted. This is the feature that caught
-- Shadowsocks, and it is here so that it never catches this.
repeats :: [ByteString.ByteString] -> Double
repeats bodies
  | null adjacent = 0
  | otherwise = count [() | (a, b) <- adjacent, a == b] / count adjacent
  where
    adjacent = concatMap (\body -> ByteString.zip body (ByteString.drop 1 body)) bodies

-- | Byte offsets holding one value in every object, beyond what chance explains.
--
-- A magic number, a version byte, a length outside the envelope or a constant
-- nonce all show up here and nowhere else.
--
-- __The raw count is the object count wearing a hat, and subtracting the chance
-- floor is what stops it being one.__ @k@ independent keystreams agree at a
-- given offset with probability @256^(1-k)@, so a world holding two drops
-- expects 512 agreements across 131 072 bytes and a world holding five expects
-- none at all. A raw count therefore separates any two worlds whose object
-- counts differ — which is the fact @drops@ already reports, in a third unit,
-- and 'declared' would have had to call it a leak that no design change could
-- ever close.
--
-- The question this was built to ask survives the correction: a field fixed by
-- the format sits at the same offset whatever the object count, so what a censor
-- could act on is agreement /above/ the floor. Zero when there are fewer than
-- two objects, because one object agrees with itself everywhere and that is an
-- artefact of the sample rather than a fact about the product.
constantPositions :: [ByteString.ByteString] -> Int
constantPositions bodies
  | held < 2 = 0
  | otherwise = max 0 (agreeing - chanceFloor)
  where
    held = length bodies
    shortest = smallest (map ByteString.length bodies)
    agreeing = length [() | at <- [0 .. shortest - 1], agrees at]
    agrees at = case map (`ByteString.index` at) bodies of
      [] -> False
      (first : rest) -> all (== first) rest
    -- Binomial, as 'Kusanagi.Veil.tolerance' is for the pairwise case: five
    -- deviations above the expectation, which chance clears about three times
    -- in ten million.
    chance = 1 / (256 ** fromIntegral (held - 1)) :: Double
    expected = fromIntegral shortest * chance :: Double
    chanceFloor = ceiling (expected + 5 * sqrt (expected * (1 - chance))) :: Int

-- | The features on which one threshold classifies every world correctly.
separating :: [[Reading]] -> [[Reading]] -> [Text]
separating left right =
  [name | name <- named left, separates (valuesOf name left) (valuesOf name right)]

named :: [[Reading]] -> [Text]
named samples = map readingName (concat (take 1 samples))

valuesOf :: Text -> [[Reading]] -> [Double]
valuesOf name samples =
  [readingValue reading | sample <- samples, reading <- sample, readingName reading == name]

-- | Whether one threshold puts every value of one group past every value of the
--   other, by more than the spread inside either group.
--
-- The margin is what makes this a statement about a rule that would keep
-- working. Two groups can fall into clean order by luck; two groups separated by
-- more than their own variation cannot, and only the second kind is something an
-- adversary could deploy against traffic it has not seen.
separates :: [Double] -> [Double] -> Bool
separates left right
  | null left || null right = False
  | otherwise = gap > spread
  where
    gap = max (minimum right - maximum left) (minimum left - maximum right)
    spread = max (spreadOf left) (spreadOf right)
    spreadOf values = maximum values - minimum values

-- | Same number of messages, three orders of magnitude apart in what they say.
volumeSaysNothing :: Door -> IO (Either String ())
volumeSaysNothing door = do
  quiet <- replicateM sides (sampleWorld door (replicate 4 1))
  loud <- replicateM sides (sampleWorld door (replicate 4 3_000))
  pure $ do
    terse <- sequence quiet
    wordy <- sequence loud
    case separating terse wordy of
      [] -> Right ()
      found ->
        Left
          ( "a host can tell four one-byte messages from four three-thousand-byte \
            \ones, and needs no key to do it:\n"
              <> report found terse wordy
          )

-- | Nothing said, against something said.
presenceSaysOnlyHowMany :: Door -> IO (Either String ())
presenceSaysOnlyHowMany door = do
  silent <- replicateM sides (sampleWorld door [])
  busy <- replicateM sides (sampleWorld door (replicate 3 200))
  pure $ do
    quiet <- sequence silent
    talking <- sequence busy
    let found = sort (separating quiet talking)
    if found == sort declared
      then Right ()
      else
        Left
          ( "what separates a silent channel from a busy one has changed.\n\
            \  written down: "
              <> shown (sort declared)
              <> "\n  measured:     "
              <> shown found
              <> "\n"
              <> report found quiet talking
              <> "\nA feature that has started separating is a new leak. One that \
                 \has stopped is a leak somebody closed, and this list is where \
                 \that gets said out loud."
          )
  where
    shown names = intercalate ", " (map Text.unpack names)

-- | What the two groups actually measured, for whoever reads the failure.
report :: [Text] -> [[Reading]] -> [[Reading]] -> String
report names left right =
  unlines
    [ "  "
      <> Text.unpack name
      <> ": "
      <> show (sort (valuesOf name left))
      <> " against "
      <> show (sort (valuesOf name right))
    | name <- names
    ]

-- | One throwaway world, measured and then deleted.
sampleWorld :: Door -> [Int] -> IO (Either String [Reading])
sampleWorld door lengths = withGround $ \ground -> do
  opened <- converse door ground lengths
  case opened of
    Left reason -> pure (Left reason)
    Right () -> Right . features <$> stored ground

-- | Opens a channel between two fresh endpoints and says these things on it.
converse :: Door -> Ground -> [Int] -> IO (Either String ())
converse door ground lengths = do
  minted <- Door.ask door writer (Door.Invite channel (waypoint ground) Door.Forever Door.both)
  case minted of
    Accepted (Invited _ invitation _) -> do
      joined <- Door.ask door reader (Door.Join invitation channel)
      case joined of
        Accepted Joined {} -> do
          mapM_ (\len -> Door.ask door writer (Door.Send channel (Text.replicate len "x"))) lengths
          pure (Right ())
        other -> pure (Left ("the channel could not be joined: " <> show other))
    other -> pure (Left ("the invitation was refused: " <> show other))
  where
    writer = siteIn ground "one"
    reader = siteIn ground "two"
    channel = ChannelName "peer"

-- | A site inside a throwaway world.
--
-- Named here rather than taken from "Kusanagi.Ground"'s cast, because these
-- endpoints have no part to play: each world is built, measured and destroyed,
-- and a name would suggest a continuity that does not exist.
siteIn :: Ground -> FilePath -> FilePath
siteIn ground name = waypoint ground <> "-" <> name
