-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | Two lies a host can tell without forging anything.
--
-- "Kusanagi.Model" already flips a byte, which is the lie a host tells by being
-- broken. These are the two it can tell while every byte it holds is a byte an
-- endpoint really wrote and really signed, so no signature check can see them:
--
-- * **Transplant.** Serve the object from one address at another. The bytes are
--   genuine and the signature verifies; only the position is a lie. This network
--   answers it with the key rather than with a check, because an address derives
--   the key its contents are sealed under — so the question this property really
--   asks is whether that derivation is load-bearing, and it would catch the day
--   somebody makes the key depend on the segment instead.
--
-- * **Vanish.** Stop serving an object. Nothing can prevent this; a store that
--   will not hand bytes over is a store that will not hand bytes over. What must
--   not follow is a reader believing *less* than it has already verified,
--   because "she never sent the cancellation" is a lie a disappearance can tell.
--
-- Both are stated as relations. Neither says what the program should print; each
-- says how two runs must stand to one another.
module Kusanagi.Lying
  ( Written (..)
  , writeSome
  , transplantIsRefused
  , historyNeverShrinks
  ) where

import Data.Text qualified as Text
import Data.Word (Word64)

import Kusanagi.Answer
  ( Address
  , Answer (..)
  , ChannelName (..)
  , Outcome (..)
  )
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, transplant, vanish)

-- | A stocked channel, and where the host put every segment of it.
data Written = Written
  { writtenReader :: FilePath
  , writtenChannel :: ChannelName
  , writtenAddresses :: [Address]
  }
  deriving stock (Eq, Show)

-- | Opens a channel and writes @count@ segments, keeping every address.
--
-- The addresses come from the sender's own @--json@, not from listing the host's
-- directory. That matters: a property that learned addresses by looking at the
-- host would silently stop testing anything the day the host's layout changed.
writeSome :: Door -> FilePath -> FilePath -> FilePath -> Int -> IO Written
writeSome door writer reader host count = do
  minted <- Door.ask door writer (Door.Invite channel host Door.Forever Door.both)
  invitation <- case minted of
    Accepted (Invited _ line _) -> pure line
    other -> fail ("the invitation was refused: " <> show other)
  joined <- Door.ask door reader (Door.Join invitation channel)
  case joined of
    Accepted Joined {} -> pure ()
    other -> fail ("the channel could not be joined: " <> show other)
  addresses <- mapM say [1 .. count]
  pure
    Written
      { writtenReader = reader
      , writtenChannel = channel
      , writtenAddresses = addresses
      }
  where
    channel = ChannelName "peer"
    say n = do
      said <- Door.ask door writer (Door.Send channel (Text.pack ("segment " <> show n)))
      case said of
        Accepted (Sent _ _ address) -> pure address
        other -> fail ("a segment was refused: " <> show other)

-- | Bytes served at an address other than their own are not read as a segment.
--
-- The two addresses are both real drops of the same stream, so nothing about
-- the object is unusual: same author, same key length, same shape, written
-- minutes apart by the same endpoint. Only the height is wrong.
--
-- Success is either a refusal or a read that stops below the transplant. Both
-- are honest; what fails is a read that hands the caller a segment sitting at a
-- height its author never put it at.
transplantIsRefused :: Door -> Ground -> Written -> IO (Either String ())
transplantIsRefused door ground written =
  case writtenAddresses written of
    (first : second : _) -> do
      before <- heightOf door written
      case before of
        Left reason -> pure (Left ("the stream did not read back first: " <> reason))
        Right seen -> do
          transplant ground second first
          after <- readingOf door written
          pure $ case after of
            Refused _ -> Right ()
            Accepted (Read _ _ height _)
              | height < seen -> Right ()
              | otherwise ->
                  Left
                    ( "a segment moved to another height was read as though it \
                      \belonged there: height "
                        <> show height
                        <> " after the move, "
                        <> show seen
                        <> " before it"
                    )
            other -> Left ("a read answered with something else: " <> show other)
    _ -> pure (Right ())

-- | A reader that has verified a height is never talked down from it.
--
-- Read once, let the host drop everything above the floor, read again. The
-- second answer may fail and it may be short, but it must not report a lower
-- height than the reader had already checked for itself — that number is what an
-- agent polls from, and a host that can lower it can replay a conversation.
historyNeverShrinks :: Door -> Ground -> Written -> IO (Either String ())
historyNeverShrinks door ground written =
  case reverse (writtenAddresses written) of
    [] -> pure (Right ())
    (top : _) -> do
      before <- heightOf door written
      case before of
        Left reason -> pure (Left ("the stream did not read back first: " <> reason))
        Right seen -> do
          vanish ground top
          after <- readingOf door written
          pure $ case after of
            Refused _ -> Right ()
            Accepted (Read _ _ height _)
              | height >= seen -> Right ()
              | otherwise ->
                  Left
                    ( "the host deleted one object and walked a reader back from \
                      \height "
                        <> show seen
                        <> " to "
                        <> show height
                        <> "; a reader that has verified a height must not \
                           \believe less than it checked"
                    )
            other -> Left ("a read answered with something else: " <> show other)

-- | The height a read reports, or why it did not report one.
heightOf :: Door -> Written -> IO (Either String (Maybe Word64))
heightOf door written = do
  answer <- readingOf door written
  pure $ case answer of
    Accepted (Read _ _ height _) -> Right height
    other -> Left (show other)

readingOf :: Door -> Written -> IO Answer
readingOf door written =
  Door.ask door (writtenReader written) (Door.Read (writtenChannel written))
