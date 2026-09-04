-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE OverloadedStrings #-}

-- | The algebraic mirror of what the door says.
--
-- This module parses and nothing else. It holds no opinion about whether an
-- answer is right, because an opinion here would be a second authority for a
-- rule that already lives in Rust.
--
-- An unknown @command@ tag is a parse failure rather than a shrug: the door has
-- changed shape, and a test that quietly treated a new shape as a refusal would
-- report green for a product it can no longer see.
module Kusanagi.Answer
  ( Answer (..)
  , Outcome (..)
  , Complaint (..)
  , Entry (..)
  , Carried (..)
  , Summary (..)
  , Address (..)
  , ChannelName (..)
  , Code (..)
  , Handle (..)
  , Invitation (..)
  , unCode
  , decodeOutcome
  , decodeComplaint
  , heard
  ) where

import Data.Aeson (FromJSON (..), eitherDecodeStrict', withObject, (.:), (.:?))
import Data.ByteString (ByteString)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Word (Word64)

-- | A handle is a public key, rendered.
newtype Handle = Handle Text
  deriving stock (Eq, Ord, Show)
  deriving newtype (FromJSON)

-- | What a channel is called on one endpoint. Names are local, not shared.
newtype ChannelName = ChannelName Text
  deriving stock (Eq, Ord, Show)
  deriving newtype (FromJSON)

-- | One line of text that admits exactly one endpoint to one channel.
newtype Invitation = Invitation Text
  deriving stock (Eq, Ord, Show)
  deriving newtype (FromJSON)

-- | An opaque place on a host where exactly one segment lives.
newtype Address = Address Text
  deriving stock (Eq, Ord, Show)
  deriving newtype (FromJSON)

-- | The stable identifier of a failure, such as @grant.revoked@.
newtype Code = Code Text
  deriving stock (Eq, Ord, Show)
  deriving newtype (FromJSON)

unCode :: Code -> Text
unCode (Code code) = code

-- | One segment, as a reader sees it.
--
-- | One segment as the door reports it: a height and what it carried.
--
-- Exactly one of @text@ and @payload@ is present, and which one is a fact about
-- the bytes rather than a choice: a payload that is valid UTF-8 survives a JSON
-- string intact, and one that is not cannot go in a string at all. Both absent
-- or both present is a door this adversary cannot read, and it says so.
data Entry = Entry
  { entryIndex :: Word64
  , entryCarried :: Carried
  }
  deriving stock (Eq, Show)

-- | What a segment carried, in the one encoding that does not lose it.
data Carried
  = AsText Text
  | -- | The exact bytes, in lowercase hexadecimal.
    AsBytes Text
  deriving stock (Eq, Show)

instance FromJSON Entry where
  parseJSON = withObject "Entry" $ \o -> do
    index <- o .: "index"
    text <- o .:? "text"
    payload <- o .:? "payload"
    carried <- case (text, payload) of
      (Just said, Nothing) -> pure (AsText said)
      (Nothing, Just bytes) -> pure (AsBytes bytes)
      _ -> fail "a segment carried both renderings or neither; the door promises exactly one"
    pure (Entry index carried)

-- | One channel, as it is listed.
data Summary = Summary
  { summaryName :: ChannelName
  , summaryStanding :: Text
  , summaryPeer :: Maybe Text
  }
  deriving stock (Eq, Show)

instance FromJSON Summary where
  parseJSON = withObject "Summary" $ \o ->
    Summary <$> o .: "name" <*> o .: "standing" <*> o .: "peer"

-- | What the program reports when it did what was asked.
data Outcome
  = Identity Handle
  | Channels [Summary]
  | Invited ChannelName Invitation Word64
  | Joined ChannelName Handle Handle
  | Sent ChannelName Word64 Address
  | -- | The channel, the handle that signed every segment reported, the
    -- verified head, and the segments themselves.
    Read ChannelName Handle (Maybe Word64) [Entry]
  | -- | The channel, and how many payloads are now waiting for a slot.
    Queued ChannelName Word64
  | -- | The channel, the slot, and the height written if one was.
    Ticked ChannelName Word64 (Maybe Word64)
  | Revoked ChannelName Text
  | -- | The channel dropped here, and the locator its drops stay at.
    Forgotten ChannelName Text
  | Examined Text Text
  | Hosted
  deriving stock (Eq, Show)

instance FromJSON Outcome where
  parseJSON = withObject "Outcome" $ \o -> do
    command <- o .: "command"
    case command :: Text of
      "identity" -> Identity <$> o .: "handle"
      "channels" -> Channels <$> o .: "channels"
      "invited" -> Invited <$> o .: "name" <*> o .: "invite" <*> o .: "expires_at"
      "joined" -> Joined <$> o .: "name" <*> o .: "handle" <*> o .: "peer"
      "sent" -> Sent <$> o .: "name" <*> o .: "index" <*> o .: "address"
      "read" -> Read <$> o .: "name" <*> o .: "author" <*> o .: "height" <*> o .: "segments"
      "queued" -> Queued <$> o .: "name" <*> o .: "waiting"
      "ticked" -> Ticked <$> o .: "name" <*> o .: "slot" <*> o .: "wrote"
      "revoked" -> Revoked <$> o .: "name" <*> o .: "step"
      "forgotten" -> Forgotten <$> o .: "name" <*> o .: "waypoint"
      "examined" -> Examined <$> o .: "waypoint" <*> o .: "tier"
      "hosted" -> pure Hosted
      other -> fail ("the door reported a command this adversary does not know: " <> Text.unpack other)

-- | What the program reports when it could not.
--
-- Every field is load-bearing: the code is what a machine acts on, and the
-- recovery is the reason a code is worth having.
data Complaint = Complaint
  { complaintCode :: Code
  , complaintMessage :: Text
  , complaintRecover :: Text
  }
  deriving stock (Eq, Show)

instance FromJSON Complaint where
  parseJSON = withObject "Complaint" $ \o ->
    Complaint <$> o .: "code" <*> o .: "error" <*> o .: "recover"

-- | The answer to one question, on whichever stream carried it.
data Answer
  = Accepted Outcome
  | Refused Complaint
  deriving stock (Eq, Show)

decodeOutcome :: ByteString -> Either String Outcome
decodeOutcome = eitherDecodeStrict'

decodeComplaint :: ByteString -> Either String Complaint
decodeComplaint = eitherDecodeStrict'

-- | The texts of a read, in order. Anything else is not a read.
heard :: Outcome -> Maybe [Text]
heard (Read _ _ _ entries) = Just (map (shown . entryCarried) entries)
  where
    shown (AsText said) = said
    shown (AsBytes bytes) = bytes
heard _ = Nothing
