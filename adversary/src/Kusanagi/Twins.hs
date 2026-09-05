-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | Two writers who are the same author.
--
-- The protocol refuses a fork by construction: one author, one height, one
-- address, and the host takes the first write. What that construction has to
-- survive is the two ways an author becomes two writers in practice — a
-- backup restored beside the original, and one site driven by several
-- processes at once. In both the reader must see one chain with no gap and no
-- height twice, the loser must hear a code, and the author must still be able
-- to write afterwards.
module Kusanagi.Twins
  ( aRestoredTwinCannotFork
  , parallelSendsNeverFork
  ) where

import Control.Concurrent (forkIO)
import Control.Concurrent.MVar (newEmptyMVar, putMVar, takeMVar)
import Control.Monad (forM, forM_)
import Data.List (sort)
import Data.Text qualified as Text
import System.FilePath ((</>))

import Kusanagi.Answer (Answer (..), Carried (..), Code (..), Complaint (..), Entry (..), Outcome (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Service qualified as Service
import Kusanagi.Ground (Ground, Site (..), siteOf)
import Kusanagi.Stage

-- | A site and its restored copy both write: the reader sees one chain, and
-- whichever twin lost is told so.
aRestoredTwinCannotFork :: Door -> Ground -> IO (Either String ())
aRestoredTwinCannotFork door ground = do
  stage <- talk door ground Alice Bob (fresh "one-author-twice")
  _ <- say door (talkWriter stage) (talkChannel stage) "before the copy"
  sealed <- Service.exporting door (talkWriter stage)
  case sealed of
    Left complaint -> pure (Left ("export was refused: " <> show complaint))
    Right (key, archive) -> do
      let twin = siteOf ground Mallory </> "twin"
      restored <- Door.ask door twin (Door.Import key archive)
      case restored of
        Accepted Imported {} -> pure ()
        other -> fail ("the archive did not restore: " <> show other)
      answers <- concurrently
        [ Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "from the original")
        , Door.ask door twin (Door.Send (talkChannel stage) "from the twin")
        ]
      _ <- Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "and the original again")
      reading <- hear door (talkReader stage) (talkChannel stage)
      pure $ do
        forM_ answers $ \answer -> case answer of
          Accepted Sent {} -> Right ()
          Refused complaint -> coded complaint
          Accepted other -> Left ("a send answered " <> show other)
        oneChain reading

-- | Eight sends at once from one site: one chain, no gap, no height twice,
-- every refusal coded, and the site still writes afterwards.
parallelSendsNeverFork :: Door -> Ground -> IO (Either String ())
parallelSendsNeverFork door ground = do
  stage <- talk door ground Alice Bob (fresh "eight-at-once")
  answers <- concurrently [Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) (Text.pack ("burst " <> show n))) | n <- [1 :: Int .. 8]]
  _ <- Door.ask door (talkWriter stage) (Door.Send (talkChannel stage) "after the burst")
  reading <- hear door (talkReader stage) (talkChannel stage)
  pure $ do
    forM_ answers $ \answer -> case answer of
      Accepted Sent {} -> Right ()
      Refused complaint -> coded complaint
      Accepted other -> Left ("a send answered " <> show other)
    entries <- entriesOf reading
    let accepted = length [() | Accepted Sent {} <- answers]
        heard = [text | Entry _ (AsText text) <- entries]
    if length entries == accepted + 1
      then Right ()
      else Left (show accepted <> " sends were accepted, one more followed, and the reader heard " <> show (length entries))
    if "after the burst" `elem` heard
      then Right ()
      else Left "the send after the burst never reached the reader: the site's record was left wrong"
    oneChain reading

-- | Runs actions at the same time and collects their results in order.
concurrently :: [IO a] -> IO [a]
concurrently actions = do
  boxes <- forM actions $ \action -> do
    box <- newEmptyMVar
    _ <- forkIO (action >>= putMVar box)
    pure box
  mapM takeMVar boxes

-- | Indices strictly increasing from zero, and the height the last of them.
oneChain :: Answer -> Either String ()
oneChain answer = do
  entries <- entriesOf answer
  let indices = map entryIndex entries
  if indices == sort indices && indices == take (length indices) [0 ..]
    then Right ()
    else Left ("the reader saw heights " <> show indices)
  case answer of
    Accepted (Read _ _ (Just height) _) | not (null indices) && height == fromIntegral (length indices - 1) -> Right ()
    Accepted (Read _ _ Nothing []) -> Right ()
    Accepted (Read _ _ height _) -> Left ("the reader reports height " <> show height <> " for " <> show (length indices) <> " segments")
    _ -> Right ()

coded :: Complaint -> Either String ()
coded complaint = case complaintCode complaint of
  Code code | Text.null code -> Left "a refusal without a code"
  _ -> Right ()
