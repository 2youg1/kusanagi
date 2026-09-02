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
  , Summary (..)
  , Address (..)
  , ChannelName (..)
  , Code (..)
  , Handle (..)
  , Invitation (..)
  , decodeOutcome
  , decodeComplaint
  , heard
  ) where

import Data.Aeson (FromJSON (..), eitherDecodeStrict', withObject, (.:))
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

-- | One segment, as a reader sees it.
data Entry = Entry
  { entryIndex :: Word64
  , entryAddress :: Address
  , entryText :: Text
  }
  deriving stock (Eq, Show)

instance FromJSON Entry where
  parseJSON = withObject "Entry" $ \o ->
    Entry <$> o .: "index" <*> o .: "address" <*> o .: "text"

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
  | Read ChannelName (Maybe Word64) [Entry]
  | Revoked ChannelName Text
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
      "read" -> Read <$> o .: "name" <*> o .: "height" <*> o .: "segments"
      "revoked" -> Revoked <$> o .: "name" <*> o .: "step"
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
heard (Read _ _ entries) = Just (map entryText entries)
heard _ = Nothing
