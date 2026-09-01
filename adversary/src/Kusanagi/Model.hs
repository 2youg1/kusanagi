-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE GADTs #-}
{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE TypeFamilies #-}

-- | What has to hold between what an endpoint was told and what it can see.
--
-- The model remembers only what a person would remember: which channels are
-- open, who is at the other end, what each side has said, who has been cut off,
-- and which invitations have been spent. It never predicts an address, a
-- digest, a signature or a ciphertext — those are rules, they already have an
-- authority in Rust, and restating one here is how a second authority begins.
--
-- What it does assert is which failure a caller sees. A stable code is part of
-- the door's contract rather than a derived value, so pinning it pins the
-- promise the product makes to the agent on the other side.
module Kusanagi.Model
  ( World (..)
  , Chan (..)
  , Mint (..)
  , Standing (..)
  , Slot
  , Action (..)
  , Kit (..)
  , Attempt
  , refusal
  , expected
  , revocationIsFinal
  ) where

import Control.Monad (when)
import Control.Monad.Reader (ReaderT, asks, liftIO)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Maybe (isJust, isNothing)
import Data.Set qualified as Set
import Data.Text (Text)
import Test.QuickCheck qualified as QC
import Test.QuickCheck.DynamicLogic (DL, DynLogicModel, action, anyActions_, failingAction, getModelStateDL)
import Test.QuickCheck.StateModel

-- Types unqualified, outcomes qualified: `Outcome` and `Action` both have a
-- `Read`, and one vocabulary in two namespaces beats two words for one thing.
import Kusanagi.Answer (ChannelName (..), Code (..), Complaint (..), Invitation)
import Kusanagi.Answer qualified as Answer
import Kusanagi.Door (Abilities (..), Door, Lifetime (..))
import Kusanagi.Door qualified as Door
import Kusanagi.Ground

-- | One channel as one endpoint holds it: a site plus the name it uses locally.
--
-- Names are local, so the same conversation is a different slot on each side and
-- the model has to carry the link between them rather than assume one.
type Slot = (Site, ChannelName)

-- | Why an endpoint is allowed on a channel.
data Standing = Root | Granted Abilities
  deriving stock (Eq, Ord, Show)

-- | An invitation that has been minted, and what became of it.
data Mint = Mint
  { mintedBy :: Slot
  , mintedGrants :: Abilities
  , mintedLiving :: Bool
  , mintedSpent :: Bool
  }
  deriving stock (Eq, Show)

-- | One side of one channel.
data Chan = Chan
  { chanStanding :: Standing
  , chanFar :: Maybe Slot
  , chanMet :: Bool
  , chanSaid :: [Text]
  , chanCut :: Bool
  }
  deriving stock (Eq, Show)

-- | Everything a person could know after a trace.
data World = World
  { worldMinted :: Map (Var Invitation) Mint
  , worldChannels :: Map Slot Chan
  }
  deriving stock (Show)

instance HasVariables World where
  getAllVariables world = Set.fromList (map Some (Map.keys (worldMinted world)))

instance HasVariables (Action World a) where
  getAllVariables (Join _ ticket _) = Set.singleton (Some ticket)
  getAllVariables _ = mempty

instance StateModel World where
  data Action World a where
    Invite :: Site -> ChannelName -> Lifetime -> Abilities -> Action World Invitation
    Join :: Site -> Var Invitation -> ChannelName -> Action World ()
    Send :: Site -> ChannelName -> Text -> Action World ()
    Read :: Site -> ChannelName -> Action World [Text]
    Revoke :: Site -> ChannelName -> Action World ()

  initialState = World {worldMinted = Map.empty, worldChannels = Map.empty}

  arbitraryAction _ world =
    QC.frequency $
      [ (3, Some <$> (Invite <$> aSite <*> aName <*> aLifetime <*> anAbility))
      , (5, Some <$> (Send <$> aSite <*> aName <*> aText))
      , (6, Some <$> (Read <$> aSite <*> aName))
      , (2, Some <$> (Revoke <$> aSite <*> aName))
      ]
        <> [ (6, Some <$> (Join <$> aSite <*> QC.elements tickets <*> aName))
           | not (null tickets)
           ]
    where
      tickets = Map.keys (worldMinted world)
      aSite = QC.elements cast
      aName = QC.elements (map ChannelName ["one", "two"])
      aLifetime = QC.frequency [(9, pure Forever), (1, pure Instantly)]
      anAbility = QC.elements [Door.both, Door.both, Door.sendOnly, Door.readOnly, Door.neither]
      aText = QC.elements ["alpha", "beta", "gamma"]

  precondition world act = attemptable world act && isNothing (refusal world act)

  -- An action the model expects to be refused is worth running, because the
  -- refusal is the promise. Running it as a negative action is how the door's
  -- error codes get tested at all.
  validFailingAction world act = attemptable world act && isJust (refusal world act)

  nextState world act ticket = case act of
    Invite site name lifetime abilities ->
      world
        { worldMinted =
            Map.insert
              ticket
              Mint
                { mintedBy = (site, name)
                , mintedGrants = abilities
                , mintedLiving = lifetime == Forever
                , mintedSpent = False
                }
              (worldMinted world)
        , worldChannels =
            Map.insert (site, name) (opened Root Nothing False) (worldChannels world)
        }
    Join site held name -> case Map.lookup held (worldMinted world) of
      Nothing -> world
      Just mint ->
        world
          { worldMinted = Map.insert held mint {mintedSpent = True} (worldMinted world)
          , worldChannels =
              Map.insert
                (site, name)
                (opened (Granted (mintedGrants mint)) (Just (mintedBy mint)) True)
                (Map.adjust (\far -> far {chanFar = Just (site, name)}) (mintedBy mint) (worldChannels world))
          }
    Send site name text ->
      alter world (site, name) (\chan -> chan {chanSaid = chanSaid chan <> [text]})
    -- A read is the only thing that teaches an inviter who accepted, so it is
    -- the only action that changes anything on the reader's side.
    Read site name -> alter world (site, name) (\chan -> chan {chanMet = True})
    Revoke site name -> alter world (site, name) (\chan -> chan {chanCut = True})

deriving stock instance Show (Action World a)

deriving stock instance Eq (Action World a)

instance DynLogicModel World

opened :: Standing -> Maybe Slot -> Bool -> Chan
opened standing far met =
  Chan {chanStanding = standing, chanFar = far, chanMet = met, chanSaid = [], chanCut = False}

alter :: World -> Slot -> (Chan -> Chan) -> World
alter world slot change = world {worldChannels = Map.adjust change slot (worldChannels world)}

-- | Whether an action can be attempted at all, quite apart from its outcome.
--
-- Only an invitation that was minted earlier in the trace can be presented; a
-- shrunk trace that dropped the minting must drop the acceptance with it.
attemptable :: World -> Action World a -> Bool
attemptable world (Join _ ticket _) = Map.member ticket (worldMinted world)
attemptable _ _ = True

-- | The refusal the model says a caller must see, if any.
--
-- The order of the guards is the order the program checks in. Getting that
-- order wrong would not weaken the property — it would make the oracle demand a
-- different failure than the one the caller is entitled to.
refusal :: World -> Action World a -> Maybe Code
refusal world = \case
  Invite site name _ _
    | occupied (site, name) -> Just (Code "kusanagi.channel_exists")
    | otherwise -> Nothing
  Join site ticket name
    | occupied (site, name) -> Just (Code "kusanagi.channel_exists")
    | otherwise -> case Map.lookup ticket (worldMinted world) of
        Nothing -> Nothing
        Just mint
          -- Found by this oracle, then fixed in Rust: an endpoint that accepted
          -- its own invitation held two local names for one stream and read its
          -- own segments back as a peer's.
          | fst (mintedBy mint) == site -> Just (Code "kusanagi.own_invitation")
          | not (mintedLiving mint) -> Just (Code "grant.expired")
          | mintedSpent mint -> Just (Code "kusanagi.invite_spent")
          | otherwise -> Nothing
  Send site name _ -> case here (site, name) of
    Nothing -> Just (Code "kusanagi.unknown_channel")
    Just chan
      | not (permits Sending (chanStanding chan)) -> Just (Code "grant.forbidden")
      | otherwise -> Nothing
  Read site name -> case here (site, name) of
    Nothing -> Just (Code "kusanagi.unknown_channel")
    Just chan
      | not (permits Reading (chanStanding chan)) -> Just (Code "grant.forbidden")
      | chanCut chan -> Just (Code "grant.revoked")
      | chanMet chan -> Nothing
      | otherwise -> case chanFar chan >>= here of
          -- Nobody has accepted, so there is nobody to have written anything.
          Nothing -> Just (Code "kusanagi.no_peer_yet")
          -- Somebody accepted, but an endpoint that may not write cannot be
          -- met either: the greeting that would introduce them is refused by
          -- the same authority that would refuse their segments.
          Just far
            | permits Sending (chanStanding far) -> Nothing
            | otherwise -> Just (Code "grant.forbidden")
  Revoke site name -> case here (site, name) of
    Nothing -> Just (Code "kusanagi.unknown_channel")
    Just chan
      | not (chanMet chan) -> Just (Code "kusanagi.no_peer_yet")
      | chanStanding chan == Root -> Nothing
      | otherwise -> Just (Code "kusanagi.cannot_revoke_root")
  where
    here slot = Map.lookup slot (worldChannels world)
    occupied = isJust . here

data Use = Sending | Reading

permits :: Use -> Standing -> Bool
permits _ Root = True
permits Sending (Granted abilities) = maySend abilities
permits Reading (Granted abilities) = mayRead abilities

-- | What a read must return: exactly what the far side said, in order.
--
-- This is the one liveness property, and it is a relation between two traces —
-- what went in on one endpoint and what came out on the other — rather than a
-- recomputation of anything.
expected :: World -> Slot -> [Text]
expected world slot =
  maybe [] chanSaid $ do
    chan <- Map.lookup slot (worldChannels world)
    far <- chanFar chan
    Map.lookup far (worldChannels world)

-- | What it takes to run a trace: a built binary and a world to run it in.
data Kit = Kit
  { kitDoor :: Door
  , kitGround :: Ground
  }

type Attempt = ReaderT Kit IO

instance RunModel World Attempt where
  type Error World Attempt = Complaint

  perform _ act look = case act of
    Invite site name lifetime abilities ->
      at site (Door.Invite name <$> host <*> pure lifetime <*> pure abilities) $ \case
        Answer.Invited _ invitation _ -> Just invitation
        _ -> Nothing
    Join site ticket name ->
      at site (pure (Door.Join (look ticket) name)) $ \case
        Answer.Joined {} -> Just ()
        _ -> Nothing
    Send site name text ->
      at site (pure (Door.Send name text)) $ \case
        Answer.Sent {} -> Just ()
        _ -> Nothing
    Read site name ->
      at site (pure (Door.Read name)) Answer.heard
    Revoke site name ->
      at site (pure (Door.Revoke name)) $ \case
        Answer.Revoked {} -> Just ()
        _ -> Nothing
    where
      host :: Attempt FilePath
      host = asks (waypoint . kitGround)
      -- The signature is load-bearing: `GADTs` brings `MonoLocalBinds` with it,
      -- and without one this helper would be pinned to whichever result type it
      -- happened to be used at first.
      at :: Site -> Attempt Door.Verb -> (Answer.Outcome -> Maybe b) -> Attempt (Either Complaint b)
      at site question project = do
        door <- asks kitDoor
        ground <- asks kitGround
        verb <- question
        answer <- liftIO (Door.ask door (siteOf ground site) verb)
        case answer of
          Answer.Refused complaint -> pure (Left complaint)
          Answer.Accepted outcome -> case project outcome of
            Just value -> pure (Right value)
            Nothing ->
              liftIO . fail $
                "the door answered a question other than the one asked: " <> show outcome

  postcondition (before, _) act _ result = case act of
    Read site name -> do
      let said = expected before (site, name)
      counterexamplePost ("heard " <> show result <> " where " <> show said <> " was said")
      pure (result == said)
    _ -> pure True

  postconditionOnFailure (before, _) act _ = \case
    Right _ -> do
      counterexamplePost ("this was accepted, and " <> show (refusal before act) <> " was owed")
      pure False
    Left complaint -> do
      counterexamplePost
        ( "refused with "
            <> show (complaintCode complaint)
            <> " where "
            <> show (refusal before act)
            <> " was owed"
        )
      pure (Just (complaintCode complaint) == refusal before act)

-- | Any prefix, one revocation, any suffix, and a read that must still fail.
--
-- Uniform random traces reach this state rarely and by accident. Naming the
-- attack and quantifying over what surrounds it is the difference between a
-- fuzzer and an adversary, and it is the reason this oracle is written in a
-- language with dynamic logic in it.
revocationIsFinal :: DL World ()
revocationIsFinal = do
  ticket <- action (Invite Alice one Forever Door.both)
  _ <- action (Join Bob ticket one)
  _ <- action (Read Alice one)
  anyActions_
  _ <- action (Revoke Alice one)
  anyActions_
  world <- getModelStateDL
  when (severed world) (failingAction (Read Alice one))
  where
    one = ChannelName "one"
    severed = maybe False chanCut . Map.lookup (Alice, one) . worldChannels
