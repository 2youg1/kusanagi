-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | What a host measures when it cannot read anything.
--
-- "Kusanagi.Lying" attacks the bytes. This module attacks the *shape* of them,
-- which is the attack a host gets for free: it never has to guess a key, break a
-- cipher or forge a signature. It weighs the parcels.
--
-- Four properties, each a relation between drops rather than an expected value:
--
-- * **One size.** Every drop is the same size, so the length of what somebody
--   said is not a thing a host holds.
-- * **Never twice.** No two drops are byte-identical, which is what a repeated
--   key or a repeated nonce looks like from outside.
-- * **No seam.** Two drops agree at about one byte in 256 and no more than the
--   noise around that. A longer agreement means structure — a header, a version,
--   a constant nonce, a keystream reused — and structure is what a detection
--   rule is made of. See 'tolerance' for what that bound reaches and what it
--   does not.
-- * **The pad is not a channel.** The same sentence sent twice leaves two drops
--   with nothing in common. If the padding were ever left unchecked, or filled
--   with anything but zeroes, the tails of those two drops would agree.
--
-- Every drop examined here is found through an address the sender's own @--json@
-- reported, never by listing the host's directory. A property that learned where
-- drops are by reading the host would quietly stop testing anything the day the
-- host's layout changed.
module Kusanagi.Veil
  ( sameSizeAlways
  , everyObjectIsOneSize
  , neverTheSameBytesTwice
  , noSharedStructure
  , theSameSentenceTwiceSharesNothing
  ) where

import Data.ByteString qualified as ByteString
import Data.List (nub, sort)
import Data.Map.Strict qualified as Map
import Data.Text (Text)
import Data.Text qualified as Text

import Kusanagi.Answer (Address, Answer (..), ChannelName (..), Outcome (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, stored, waypoint)

-- | How much two drops of @n@ bytes may agree before the agreement means
-- something.
--
-- Two independent ChaCha20 keystreams agree at one byte in 256, so the count of
-- agreeing positions is binomial: it expects @n \/ 256@ and has a standard
-- deviation of @sqrt (n * 255 \/ 256^2)@. The threshold is five deviations above
-- the expectation, which one pair clears by chance about three times in ten
-- million while a run compares barely a dozen pairs.
--
-- __It has to be a function of @n@, and it used to be the constant 64.__ That
-- constant was calibrated when a drop was 4 096 bytes and chance explained
-- sixteen agreements. ML-DSA-87 pushed a drop to 131 072 bytes, chance began
-- explaining 512, and three properties here failed on every run against a build
-- with nothing wrong with it — the observed counts, 490 to 536, sit inside
-- 512 ± 1.1 deviations. The noise floor grows with the square root of the drop,
-- so the slack above it must too, and the next change of signature scheme now
-- needs no edit here.
--
-- What this catches is agreement spread across the whole drop and larger than
-- the noise: a reused keystream, a constant nonce, a tail left in the clear.
-- What it does not catch is a short fixed field — a four-byte tag at the same
-- offset in every drop lifts the count by four, far under a noise floor of 113.
-- A leading header is caught by 'prefixTolerance'; a short one in the middle is
-- watched by nothing here today, and the property that would watch it compares
-- which /positions/ agree across many pairs rather than how many agree in one.
tolerance :: Int -> Int
tolerance n = expected + 5 * deviation
  where
    expected = n `div` 256
    -- `sqrt` on a `Double` and back: the count is an integer, and the deviation
    -- only has to be right to a byte.
    deviation = ceiling (sqrt (fromIntegral n * 255 / 65536 :: Double)) :: Int

-- | The longest run of equal leading bytes two drops may share.
--
-- Zero would be too strict: two random strings share a first byte once in 256.
-- Four is out of reach by chance across the handful of drops a test writes, and
-- shorter than any header anybody would add.
prefixTolerance :: Int
prefixTolerance = 4

-- | The channel every property here opens. Names are local, so one will do.
channel :: ChannelName
channel = ChannelName "peer"

-- | Every drop is the same size, whatever it carries.
--
-- The messages differ by three orders of magnitude on purpose. A host that can
-- tell a one-byte remark from a three-thousand-byte one holds the shape of the
-- conversation, and a length profile survives encryption — it is how a censor
-- recognises a login, a photograph, a refusal.
sameSizeAlways :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
sameSizeAlways door ground writer reader =
  written door ground writer reader lengths $ \bodies ->
    case nub (sort (map ByteString.length bodies)) of
      [_] -> Right ()
      sizes ->
        Left
          ( "messages of lengths "
              <> show lengths
              <> " produced drops of sizes "
              <> show sizes
              <> "; a host that can measure an object can measure what was said"
          )
  where
    lengths = [1, 7, 60, 500, 3000]

-- | Everything the host is holding is one size, including what nobody reported.
--
-- The properties above judge the drops a sender named, which keeps a greeting or
-- any other protocol traffic out of the sample so that it cannot dilute them.
-- This one takes the opposite side deliberately: a host does not know which
-- objects were announced, so what it weighs is the whole store. An introduction
-- that is shorter than a message — or a build that grows one without growing the
-- other — marks the first object of every conversation, and the first object of
-- a conversation is the one that says a conversation began.
everyObjectIsOneSize :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
everyObjectIsOneSize door ground writer reader = do
  opened <- open door ground writer reader
  case opened of
    Left reason -> pure (Left reason)
    Right () -> do
      _ <- say door writer (Text.replicate 300 "x")
      _ <- say door reader "a short answer"
      held <- stored ground
      let bodies = map snd held
      pure $ case (length bodies, nub (sort (map ByteString.length bodies))) of
        (0, _) -> Left "the host is holding nothing after a conversation"
        (_, [_]) ->
          case [reason | (left, right) <- pairs bodies, Just reason <- [apart left right]] of
            [] -> Right ()
            reasons -> Left (unlines reasons)
        (_, sizes) ->
          Left
            ( "the host is holding objects of sizes "
                <> show sizes
                <> "; one of them is the introduction, and a size that stands out "
                <> "marks where every conversation begins"
            )

-- | No two drops are the same bytes.
--
-- Invisible from the address side, and not subtle: a key or a nonce reused
-- across two drops makes identical plaintexts produce identical ciphertexts, so
-- a host that spots two equal objects has learnt that the same thing was said
-- twice without opening either.
neverTheSameBytesTwice :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
neverTheSameBytesTwice door ground writer reader =
  written door ground writer reader (replicate 6 32) $ \bodies ->
    if length (nub bodies) == length bodies
      then Right ()
      else Left "two drops are byte-identical; a key or a nonce was used twice"

-- | No two drops share more than chance.
--
-- Any position where drops agree is a rule a censor can write, and finding it
-- costs one pass over a store. This is the property that fails on the day
-- somebody adds a magic number, a version byte or a length outside the envelope.
noSharedStructure :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
noSharedStructure door ground writer reader =
  written door ground writer reader [10, 200, 1500, 40] $ \bodies ->
    case [reason | (left, right) <- pairs bodies, Just reason <- [apart left right]] of
      [] -> Right ()
      reasons -> Left (unlines reasons)

-- | The same sentence, sent twice, leaves nothing in common behind.
--
-- Two heights, one key each. Resemblance would mean the derivation is not doing
-- its work; agreement confined to the tail would mean the padding is carrying
-- something — a counter, a build identifier, whatever a well-meaning patch put
-- there — and that padding is a covert channel nothing else would notice.
theSameSentenceTwiceSharesNothing ::
  Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
theSameSentenceTwiceSharesNothing door ground writer reader = do
  opened <- open door ground writer reader
  case opened of
    Left reason -> pure (Left reason)
    Right () -> do
      addresses <- traverse (const (say door writer sentence)) [1 :: Int, 2]
      held <- stored ground
      pure $ case bodiesAt held (concat addresses) of
        [first, second] -> maybe (Right ()) Left (apart first second)
        other ->
          Left
            ( "two drops were written and "
                <> show (length other)
                <> " were found at the addresses the sender reported"
            )
  where
    sentence = Text.replicate 40 "the same thing "

-- | Why two drops are too alike, if they are.
apart :: ByteString.ByteString -> ByteString.ByteString -> Maybe String
apart left right
  | agreement > tolerance width =
      Just
        ( "two drops agree at "
            <> show agreement
            <> " byte positions, and chance explains up to "
            <> show (tolerance width)
            <> "; that is structure, and structure is a detection rule"
        )
  | shared > prefixTolerance =
      Just
        ( "two drops share a "
            <> show shared
            <> "-byte prefix; a header that long is all a classifier needs"
        )
  | otherwise = Nothing
  where
    width = min (ByteString.length left) (ByteString.length right)
    paired = ByteString.zip left right
    agreement = length [() | (a, b) <- paired, a == b]
    shared = length (takeWhile id [a == b | (a, b) <- paired])

pairs :: [a] -> [(a, a)]
pairs items = [(x, y) | (index, x) <- zip [0 :: Int ..] items, y <- drop (index + 1) items]

-- | Opens a channel, says one message of each length, and judges what was left.
--
-- The judge is handed exactly the drops the sender named, so a greeting or any
-- other traffic the protocol writes on its own is out of the sample and cannot
-- make a property pass by diluting it.
written ::
  Door ->
  Ground ->
  FilePath ->
  FilePath ->
  [Int] ->
  ([ByteString.ByteString] -> Either String ()) ->
  IO (Either String ())
written door ground writer reader lengths judge = do
  opened <- open door ground writer reader
  case opened of
    Left reason -> pure (Left reason)
    Right () -> do
      addresses <- traverse (say door writer . flip Text.replicate "x") lengths
      held <- stored ground
      let bodies = bodiesAt held (concat addresses)
      pure $
        if length bodies == length lengths
          then judge bodies
          else
            Left
              ( "the sender reported "
                  <> show (length (concat addresses))
                  <> " addresses and the host is holding "
                  <> show (length bodies)
                  <> " of them"
              )

-- | Opens the channel both sides then use.
open :: Door -> Ground -> FilePath -> FilePath -> IO (Either String ())
open door ground writer reader = do
  minted <- Door.ask door writer (Door.Invite channel (waypoint ground) Door.Forever Door.both)
  case minted of
    Accepted (Invited _ invitation _) -> do
      joined <- Door.ask door reader (Door.Join invitation channel)
      pure $ case joined of
        Accepted Joined {} -> Right ()
        other -> Left ("the channel could not be joined: " <> show other)
    other -> pure (Left ("the invitation was refused: " <> show other))

-- | Says one thing and reports where the sender says it put it.
say :: Door -> FilePath -> Text -> IO [Address]
say door writer text = do
  said <- Door.ask door writer (Door.Send channel text)
  pure $ case said of
    Accepted (Sent _ _ address) -> [address]
    _ -> []

-- | The bytes at each of these addresses, in the order the addresses were given.
bodiesAt ::
  [(Address, ByteString.ByteString)] -> [Address] -> [ByteString.ByteString]
bodiesAt held wanted =
  [body | address <- wanted, Just body <- [Map.lookup address (Map.fromList held)]]
