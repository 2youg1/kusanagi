-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}

-- | Two journeys through the window, end to end (H7): a private conversation
-- and a room, each started by a person at the sheet, joined by Bob at his
-- terminal, spoken in from the composer and heard back on the next refresh.
--
-- Black box on both sides: the window is driven through the automation
-- server and read through its snapshot; Bob is the CLI. Neither side is
-- linked, and what the window drawnAfterRefresh is the only evidence accepted.
module Kusanagi.Journey
  ( aConversationStartsInTheWindow
  , aRoomIsFoundedInTheWindow
  ) where

import Control.Concurrent (threadDelay)
import Data.Text (Text)
import System.Exit (ExitCode (..))
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text

import Kusanagi.Answer (Answer (..), ChannelName (..), Entry (..), Carried (..), Invitation (..), Outcome (..))
import Kusanagi.Automation
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Glass (awaiting, byName, prepared, running)
import Kusanagi.Ground (Ground, Site (..), siteOf, waypoint)
import Kusanagi.Stage (entriesOf, hear, say)

-- | Drives the invite sheet to a minted line: name, waypoint, optionally the
-- room switch, then Mint; answers the `kusanagi2:` line the window drawnAfterRefresh.
minted :: Glass -> Ground -> Text -> Bool -> IO (Either String Text)
minted glass ground name room = do
  first <- widgets <$> awaiting glass ["新邀请", "New invitation"]
  case byName ["新邀请", "New invitation"] first of
    Nothing -> pure (Left ("the window never showed an invite button" <> seen (Text.unlines (map widgetName first))))
    Just new -> do
      _ <- press glass new
      threadDelay 500_000
      shot <- snapshot glass
      let sheet = widgets shot
      case (named "textbox" ["名字", "Name"] sheet, named "textbox" ["Waypoint"] sheet) of
        (Just field, Just host) -> do
          _ <- setText glass field (Text.unpack name)
          _ <- setText glass host (waypoint ground)
          switched <-
            if not room
              then pure (Right ())
              else case byName ["建房间而不是通道:受邀的每个人都能读到彼此", "A room, not a channel: everybody invited reads everybody"] sheet of
                Nothing -> pure (Left ("the invite sheet has no room switch" <> seen shot))
                Just toggle -> Right () <$ press glass toggle
          case switched of
            Left reason -> pure (Left reason)
            Right () ->
              case named "button" ["生成", "Mint"] sheet of
                Nothing -> pure (Left "the invite sheet has no mint button")
                Just mint -> do
                  _ <- press glass mint
                  threadDelay 4_000_000
                  later <- snapshot glass
                  let after = widgets later
                  case [widgetName w | w <- after, "kusanagi2:" `Text.isPrefixOf` widgetName w] of
                    (line : _) -> do
                      mapM_ (press glass) (byName ["完成", "Done"] after)
                      threadDelay 500_000
                      pure (Right line)
                    [] -> pure (Left ("the window did not show a minted line" <> seen later))
        _ -> pure (Left "the invite sheet is missing a field")

-- | Opens the row called `name` and types `text` into the composer labelled
-- `composer`, then presses `button`.
spoken :: Glass -> Text -> [Text] -> [Text] -> String -> IO (Either String ())
spoken glass name composer button text = do
  shot <- snapshot glass
  case named "listitem" [name] (widgets shot) of
    Nothing -> pure (Left ("the rail does not list " <> Text.unpack name <> seen shot))
    Just row -> do
      _ <- press glass row
      threadDelay 3_000_000
      opened <- snapshot glass
      let page = widgets opened
      case (named "textbox" composer page, named "button" button page) of
        (Just field, Just send) -> do
          _ <- setText glass field text
          _ <- press glass send
          threadDelay 4_000_000
          pure (Right ())
        _ -> pure (Left ("the page has no composer or send button" <> seen opened))

-- | Whether `text` is drawn within one poll interval and a half: the window
-- asks the host every twenty seconds, and nothing here hurries it.
drawnAfterRefresh :: Glass -> Text -> IO Bool
drawnAfterRefresh glass text = go (30 :: Int)
  where
    go attempts = do
      drawn <- widgets <$> snapshot glass
      if any ((text `Text.isInfixOf`) . widgetName) drawn
        then pure True
        else if attempts == 0 then pure False else threadDelay 1_000_000 >> go (attempts - 1)

said :: Either String [Entry] -> [Text]
said = either (const []) (map shown)
  where
    shown entry = case entryCarried entry of
      AsText text -> text
      AsBytes hex -> hex

-- | A person mints in the window, Bob joins at his terminal, the person
-- writes in the composer and Bob reads it; Bob replies and the window drawnAfterRefresh it.
aConversationStartsInTheWindow :: Door -> Ground -> IO (Either String ())
aConversationStartsInTheWindow door ground =
  prepared door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> running glass $
      minted glass ground "jie" False >>= \case
        Left reason -> pure (Left reason)
        Right line ->
          Door.ask door (siteOf ground Bob) (Door.Join (Invitation line) (ChannelName "win")) >>= \case
            Accepted Joined {} -> do
              typed <- spoken glass "jie" ["消息", "Message"] ["发送", "Send"] "from the window: seventeen"
              heard <- said . entriesOf <$> hear door (siteOf ground Bob) (ChannelName "win")
              _ <- say door (siteOf ground Bob) (ChannelName "win") "from the terminal: eighteen"
              drawn <- drawnAfterRefresh glass "from the terminal: eighteen"
              pure $ do
                typed
                if "from the window: seventeen" `elem` heard then Right () else Left ("Bob read " <> show heard)
                if drawn then Right () else Left "the window did not show Bob's reply after a refresh"
            other -> pure (Left ("Bob could not join with the line the window showed: " <> show other))

-- | The same journey through a room: founded at the sheet with the room
-- switch, joined by Bob, one line each way.
aRoomIsFoundedInTheWindow :: Door -> Ground -> IO (Either String ())
aRoomIsFoundedInTheWindow door ground =
  prepared door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> running glass $
      minted glass ground "hall" True >>= \case
        Left reason -> pure (Left reason)
        Right line -> do
          let bob = siteOf ground Bob
              root = ["--root", bob, "--json"]
          joined <- Door.typed door (root <> ["room-join", "--name", "-"]) (Just (Text.encodeUtf8 ("hall\n" <> line)))
          if Door.typedStatus joined /= ExitSuccess
            then pure (Left ("Bob could not join the room: " <> show (Door.typedOut joined)))
            else do
              typed <- spoken glass "hall" ["广播", "Broadcast"] ["发给所有人", "Send to all"] "room from the window"
              -- The founder's window admits Bob on its own read; Bob then hears the room.
              _ <- drawnAfterRefresh glass "room from the window"
              reading <- Door.typed door (root <> ["room-read", "--name", "-"]) (Just "hall\n")
              _ <- Door.typed door (root <> ["room-send", "--name", "-"]) (Just "hall\nroom from the terminal")
              drawn <- drawnAfterRefresh glass "room from the terminal"
              pure $ do
                typed
                if "room from the window" `Text.isInfixOf` Text.decodeUtf8 (Door.typedOut reading) then Right () else Left ("Bob's room read did not carry the line: " <> show (Door.typedOut reading))
                if drawn then Right () else Left "the window did not show Bob's room line after a refresh"
