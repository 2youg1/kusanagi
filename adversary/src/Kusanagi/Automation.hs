-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}

-- | Driving the window from outside: the automation server, its snapshots,
-- and the widgets they describe.
--
-- Everything here is plumbing shared by every claim in "Kusanagi.Glass": how
-- a `native automate` command is run and read back as bytes, how a snapshot
-- is asked for and parsed, and how a widget is found and pressed. No claim
-- about privacy lives here.
module Kusanagi.Automation
  ( Glass (..)
  , Widget (..)
  , automationDir
  , capture
  , lenient
  , automate
  , snapshot
  , widgets
  , named
  , press
  , setText
  , seen
  , noErrorEvent
  ) where

import Control.Concurrent (threadDelay)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.Maybe (listToMaybe, mapMaybe)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Exit (ExitCode (..))
import System.FilePath ((</>))
import System.IO (hSetBinaryMode)
import System.Process (CreateProcess (..), StdStream (..), proc, waitForProcess, withCreateProcess)

data Glass = Glass
  { glassDir :: FilePath
  , glassAppData :: FilePath
  , glassHome :: FilePath
  , -- | The site the window opens: the CLI's default root under `LOCALAPPDATA`.
    glassSite :: FilePath
  }

automationDir :: Glass -> FilePath
automationDir glass = glassDir glass </> ".zig-cache" </> "native-sdk-automation"

-- | One command, its bytes captured whole. Bytes rather than `String`: the
-- shell tools on a Chinese Windows answer in the console code page, which
-- the locale decoder refuses.
capture :: Maybe FilePath -> FilePath -> [String] -> IO (ExitCode, ByteString, ByteString)
capture directory binary arguments =
  withCreateProcess (proc binary arguments) {cwd = directory, std_in = NoStream, std_out = CreatePipe, std_err = CreatePipe} $ \_ out err handle ->
    case (out, err) of
      (Just outHandle, Just errHandle) -> do
        hSetBinaryMode outHandle True
        hSetBinaryMode errHandle True
        reported <- ByteString.hGetContents outHandle
        complained <- ByteString.hGetContents errHandle
        status <- waitForProcess handle
        pure (status, reported, complained)
      _ -> fail "no pipes"

lenient :: ByteString -> String
lenient = Text.unpack . Text.decodeUtf8With (\_ _ -> Just '\xfffd')

-- | One `native automate` command against the running window.
automate :: Glass -> [String] -> IO (Either String String)
automate glass arguments = do
  (status, out, err) <- capture (Just (glassDir glass)) "native" ("automate" : arguments)
  pure $ case status of
    ExitSuccess -> Right (lenient out)
    ExitFailure code -> Left ("native automate " <> unwords arguments <> " failed with " <> show code <> ": " <> lenient err)

-- | The widget tree as the automation server publishes it, after asking for a
-- fresh one. The first request after `wait` can race the publisher, so it is
-- asked again for a moment before giving up.
snapshot :: Glass -> IO Text
snapshot glass = go (5 :: Int)
  where
    go attempts =
      automate glass ["snapshot"] >>= \case
        Right _ -> Text.decodeUtf8With (\_ _ -> Just '\xfffd') <$> ByteString.readFile (automationDir glass </> "snapshot.txt")
        Left reason
          | attempts > 0 -> threadDelay 300_000 >> go (attempts - 1)
          | otherwise -> fail reason

data Widget = Widget
  { widgetId :: Text
  , widgetRole :: Text
  , widgetName :: Text
  , widgetActions :: [Text]
  }

widgets :: Text -> [Widget]
widgets = mapMaybe one . Text.lines
  where
    one line = do
      rest <- snd <$> stripInfix "widget @w1/glass-canvas#" line
      let (ident, after) = Text.breakOn " " rest
      role <- field "role=" after
      name <- quoted "name=\"" after
      let actions = maybe [] (Text.splitOn "," . Text.takeWhile (/= ']')) (snd <$> stripInfix "actions=[" after)
      pure (Widget ident role name actions)
    field key text = Text.takeWhile (/= ' ') . snd <$> stripInfix key text
    quoted key text = Text.takeWhile (/= '"') . snd <$> stripInfix key text
    stripInfix needle haystack =
      let (before, found) = Text.breakOn needle haystack
       in if Text.null found then Nothing else Just (before, Text.drop (Text.length needle) found)

-- | The first widget of a role carrying one of the names — the English and
-- the Chinese label of the same button, whichever language the window chose.
named :: Text -> [Text] -> [Widget] -> Maybe Widget
named role names = listToMaybe . filter (\w -> widgetRole w == role && widgetName w `elem` names)

press :: Glass -> Widget -> IO (Either String String)
press glass widget = automate glass ["widget-click", "glass-canvas", Text.unpack (widgetId widget)]

setText :: Glass -> Widget -> String -> IO (Either String String)
setText glass widget value = automate glass ["widget-action", "glass-canvas", Text.unpack (widgetId widget), "set_text", value]

-- | What the window showed, for a failure message: every text, button and
-- link by name, so a red cell says what was on screen.
seen :: Text -> String
seen shown = " — on screen: " <> show [widgetRole w <> ":" <> Text.take 40 (widgetName w) | w <- widgets shown, widgetRole w `elem` ["text", "button", "link", "listitem"]]

noErrorEvent :: Text -> Either String ()
noErrorEvent shown
  | "error event=" `Text.isInfixOf` shown = Left "the runtime reported an error event"
  | otherwise = Right ()

