-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | The same experiment as "Kusanagi.Discriminator", from the other position.
--
-- A host weighs objects. A carrier cannot: it holds nothing, opens nothing, and
-- under TLS does not even see an address. What it has is a list of moments, and
-- the question is whether one threshold on one number built from those moments
-- sorts the worlds into the two groups they were built as.
--
-- **Two positions, two lists of declared leaks, and they close on different
-- days.** The fixed-size envelope closed the host's size features years before
-- anything was done about rhythm; the public slot of `Roadmap.md` §I3 closes
-- rhythm without touching a single byte the host holds. Merging the two lists
-- would hide which mechanism was responsible for which line.
module Kusanagi.Tempo
  ( timings
  , everyRequestIsSeen
  , volumeKeepsTime
  , presenceSaysOnlyWhatIsWrittenDown
  ) where

import Control.Monad (replicateM)
import Data.List (sort)
import Data.Text (Text)
import Data.Text qualified as Text

import Kusanagi.Discriminator
  ( Reading (..)
  , Sample (..)
  , report
  , sampleWorld
  , separating
  , sides
  )
import Kusanagi.Door (Door)
import Kusanagi.Relay (Observation (..))

-- | The timing features that are allowed to separate silence from conversation.
--
-- **Measured, not predicted.** @gap.burst@ came out at 3, 3, 3, 3 against 12,
-- 12, 12, 12 — no overlap and no spread at all, because it is a count rather
-- than a duration. @gap.median@ did not separate on any run: both worlds pay the
-- same process start for each command, so the middle of the distribution is the
-- same in a world that says nothing as in one that says three things. Writing
-- down a leak that does not happen is as wrong as leaving out one that does,
-- because the assertion below is an equality in both directions.
--
-- What it comes to: a command is a burst of requests, so the number of bursts is
-- the number of commands, which is the conversation. Nothing in the current
-- design can change that — an endpoint reaches the host exactly when its user
-- reaches it. **This is not the same mistake as the object count in
-- @Kusanagi.Discriminator@**, which was one fact reported in a second unit and
-- therefore not declarable: the carrier holds nothing and counts nothing else,
-- so this is its only channel, and the public slot of `Roadmap.md` §I3 closes it
-- outright by making the request times a function of the clock instead of of
-- what anybody said. The day that lands, this list becomes empty and the
-- property below is what says so.
declared :: [Text]
declared = ["gap.burst"]

-- | One number a carrier could put a threshold on.
--
-- The middle of the distribution and the clustering. A public slot flattens both
-- at once — every slot carries exactly one request whether or not anybody is
-- talking — so a mechanism that moved only one of them would show up here as one
-- line that stayed.
--
-- __The longest gap was measured, found to be an extremum, and removed.__ It
-- separated on about one run in three, at 41–44 ms against 48–55 ms, and the
-- reason is arithmetic rather than anything about this product: the largest of
-- twelve draws exceeds the largest of three even when both come from the same
-- distribution, and the number of draws is the number of requests, which
-- 'declared' already reports. Declaring it would have claimed a leak that no
-- design change could close, which is the mistake
-- @Kusanagi.Discriminator.constantPositions@ records at the same rank in the
-- other feature set. **A statistic whose value depends on how many samples it
-- saw is a sample count in disguise.** Anything that grows with the sample — a
-- maximum, a range, a total — belongs here only after that dependence has been
-- taken out of it, and for a maximum over three samples there is no honest way
-- to do that.
timings :: [Observation] -> [Reading]
timings seen =
  [ Reading "gap.median" (middle gaps)
  , Reading "gap.burst" (fromIntegral (length [() | gap <- gaps, gap < 0.1]))
  ]
  where
    moments = map observedAt seen
    gaps = zipWith subtract moments (drop 1 moments)

middle :: [Double] -> Double
middle [] = 0
middle values = case drop (length values `div` 2) (sort values) of
  (at : _) -> at
  [] -> 0

-- | Whether anything was measured at all.
--
-- **This runs before the other two and exists because of how they fail.** A
-- relay that recorded nothing gives every world an empty feature vector, no
-- feature separates anything, and both properties below go green while testing
-- precisely nothing. So the first question is not about the product: it is
-- whether this instrument is plugged in.
everyRequestIsSeen :: Door -> IO (Either String ())
everyRequestIsSeen door = do
  sampled <- sampleWorld door [40, 40]
  pure $ do
    world <- sampled
    let seen = sampleSeen world
    if length seen < 2
      then
        Left
          ( "the relay carried a whole conversation and recorded "
              <> show (length seen)
              <> " requests. Every timing property below is green for this reason \
                 \and not for a better one."
          )
      else case [path | Observation {observedPath = path} <- seen, not (drop' path)] of
        [] -> Right ()
        strange ->
          Left
            ( "the endpoint asked the host for something that is not a drop: "
                <> show (take 4 strange)
            )
  where
    drop' path = "/d/" `Text.isPrefixOf` path

-- | Same number of messages, three orders of magnitude apart in what they say.
--
-- A fixed-size envelope makes them identical to the host. It should make them
-- identical to the carrier too, and a difference here would mean the size had
-- come back as a duration — a longer message taking longer to send is exactly
-- the leak the padding was bought to prevent.
volumeKeepsTime :: Door -> IO (Either String ())
volumeKeepsTime door = do
  quiet <- replicateM sides (kept door (replicate 4 1))
  loud <- replicateM sides (kept door (replicate 4 3_000))
  pure $ do
    terse <- sequence quiet
    wordy <- sequence loud
    case separating terse wordy of
      [] -> Right ()
      found ->
        Left
          ( "a carrier can tell four one-byte messages from four three-thousand-byte \
            \ones by their rhythm alone:\n"
              <> report found terse wordy
          )

-- | Nothing said, against something said, as a carrier hears it.
presenceSaysOnlyWhatIsWrittenDown :: Door -> IO (Either String ())
presenceSaysOnlyWhatIsWrittenDown door = do
  silent <- replicateM sides (kept door [])
  busy <- replicateM sides (kept door (replicate 3 200))
  pure $ do
    quiet <- sequence silent
    talking <- sequence busy
    let found = sort (separating quiet talking)
    if found == sort declared
      then Right ()
      else
        Left
          ( "what a carrier hears between a silent channel and a busy one has changed.\n\
            \  written down: "
              <> shown (sort declared)
              <> "\n  measured:     "
              <> shown found
              <> "\n"
              <> report found quiet talking
              <> "\nA timing feature that has started separating is a new leak on the \
                 \path. One that has stopped is a slot somebody built, and this list \
                 \is where that gets said out loud."
          )
  where
    shown names = Text.unpack (Text.intercalate ", " names)

-- | One world, as its carrier heard it.
kept :: Door -> [Int] -> IO (Either String [Reading])
kept door lengths = fmap (timings . sampleSeen) <$> sampleWorld door lengths
