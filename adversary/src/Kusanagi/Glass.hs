-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}
{-# LANGUAGE ScopedTypeVariables #-}

-- | The window, driven from outside, against what a peer can put in it.
--
-- `glass` renders a peer's bytes as markdown. D-18 rules that rendering never
-- causes I/O: an image is drawn as its alt text, a link has nothing bound to
-- it, and nothing a peer sends reaches the disk, the clipboard or the network
-- from this machine. Those are the claims a rogue peer would test, so they are
-- tested the way a rogue peer would: a listener on this machine counts
-- connections, the automation server reports what was drawn, and the disk and
-- clipboard are read back afterwards. Nothing here believes "by construction".
--
-- The window is launched with `LOCALAPPDATA` and `USERPROFILE` pointed into a
-- throwaway directory, so the real site and the real preferences are never
-- touched. The whole module skips itself when the window has not been built
-- (`native build -Dautomation=true -Dtrace=off` in `glass/`) or the `native`
-- CLI is not on PATH — CI never builds the GUI (Roadmap fact 21), so this is
-- a gate for the machine that ships, not for the machine that merges.
module Kusanagi.Glass
  ( available
  , aRemoteImageIsNeverFetched
  , aLinkCannotBePressed
  , controlBytesAreShownAsHex
  , theDiskHoldsNoPeer
  , theClipboardWaitsForAHand
  ) where

import Control.Concurrent (threadDelay)
import Control.Exception (SomeException, bracket, try)
import Control.Monad (forM, forM_, unless)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.Maybe (isJust, listToMaybe, mapMaybe)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Directory
  ( canonicalizePath
  , copyFile
  , createDirectoryIfMissing
  , doesDirectoryExist
  , doesFileExist
  , findExecutable
  , listDirectory
  , removePathForcibly
  )
import System.Environment (getEnvironment)
import System.Exit (ExitCode (..))
import System.FilePath (takeDirectory, takeFileName, (</>))
import System.IO (IOMode (WriteMode), hClose, hSetBinaryMode, openFile)
import System.Process
  ( CreateProcess (..)
  , StdStream (..)
  , createProcess
  , proc
  , terminateProcess
  , waitForProcess
  , withCreateProcess
  )

import Kusanagi.Answer (Answer (..), ChannelName (..), Outcome (..))
import Kusanagi.Door (Door (..), Typed (..))
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..), siteOf, waypoint)
import Kusanagi.Listener (Script (..), connections, locatorOf, withListener)
import Kusanagi.Stage (say)

-- | Where the window is, when it has been built and can be driven.
available :: IO (Maybe FilePath)
available = do
  dir <- canonicalizePath (".." </> "glass")
  built <- doesFileExist (dir </> "zig-out" </> "bin" </> "glass.exe")
  native <- findExecutable "native"
  pure (if built && isJust native then Just dir else Nothing)

notBuilt :: String
notBuilt = "the window is not built; run `native build -Dautomation=true -Dtrace=off` in glass/"

data Glass = Glass
  { glassDir :: FilePath
  , glassAppData :: FilePath
  , glassHome :: FilePath
  , -- | The site the window opens: the CLI's default root under `LOCALAPPDATA`.
    glassSite :: FilePath
  }

-- | The window's site and Bob, peered, with the window not yet running.
--
-- A throwaway home for one run, with the binary under test placed beside the
-- window — it runs the `kusanagi.exe` next to itself, and a stale one there
-- once cost half an hour. The window's site invites under the name @lin@,
-- which is what the rail shows; Bob joins under the name @me@.
staged :: Door -> Ground -> IO (Either String Glass)
staged door@(Door binary) ground =
  available >>= \case
    Nothing -> pure (Left notBuilt)
    Just dir -> do
      let root = takeDirectory (waypoint ground)
          appdata = root </> "appdata"
          home = root </> "home"
      createDirectoryIfMissing True appdata
      createDirectoryIfMissing True home
      -- Copied only when it differs: a running window's verb holds the file
      -- for a moment, and replacing an identical file buys nothing.
      let beside = dir </> "zig-out" </> "bin" </> "kusanagi.exe"
      there <- doesFileExist beside
      same <- if there then (==) <$> ByteString.readFile binary <*> ByteString.readFile beside else pure False
      unless same (copyFile binary beside)
      let glass = Glass {glassDir = dir, glassAppData = appdata, glassHome = home, glassSite = appdata </> "kusanagi"}
      Door.ask door (glassSite glass) (Door.Invite (ChannelName "lin") (waypoint ground) Door.Forever Door.both) >>= \case
        Accepted (Invited _ invitation _) ->
          Door.ask door (siteOf ground Bob) (Door.Join invitation (ChannelName "me")) >>= \case
            Accepted Joined {} -> pure (Right glass)
            other -> pure (Left ("Bob could not join: " <> show other))
        other -> pure (Left ("the window's site could not invite: " <> show other))

-- | What Bob says is what the window renders.
bobSays :: Door -> Ground -> Text -> IO ()
bobSays door ground text = () <$ say door (siteOf ground Bob) (ChannelName "me") text

-- | Runs the window over `act`, and kills it afterwards whatever happened.
running :: Glass -> IO a -> IO a
running glass act = do
  _ <- capture Nothing "taskkill" ["/F", "/IM", "glass.exe"]
  threadDelay 500_000
  clearing (5 :: Int)
  surroundings <- getEnvironment
  let ours = [("LOCALAPPDATA", glassAppData glass), ("USERPROFILE", glassHome glass), ("HOME", glassHome glass)]
      environment = ours <> filter ((`notElem` map fst ours) . fst) surroundings
  out <- openFile (glassHome glass </> "glass-out.log") WriteMode
  err <- openFile (glassHome glass </> "glass-err.log") WriteMode
  let launch =
        createProcess
          (proc (glassDir glass </> "zig-out" </> "bin" </> "glass.exe") [])
            { cwd = Just (glassDir glass)
            , env = Just environment
            , std_in = NoStream
            , std_out = UseHandle out
            , std_err = UseHandle err
            }
      stop (_, _, _, handle) = do
        terminateProcess handle
        _ <- waitForProcess handle
        hClose out
        hClose err
  bracket launch stop $ \_ -> do
    _ <- automate glass ["wait", "--timeout-ms", "20000"]
    act
  where
    -- The window just killed may still hold the automation files for a moment.
    clearing attempts =
      try (removePathForcibly (automationDir glass)) >>= \case
        Right () -> pure ()
        Left (_ :: SomeException)
          | attempts > 0 -> threadDelay 500_000 >> clearing (attempts - 1)
          | otherwise -> removePathForcibly (automationDir glass)

-- | The window open on the channel, with the thread drawn.
opened :: Glass -> (Text -> IO (Either String a)) -> IO (Either String a)
opened glass act = running glass $ do
  first <- snapshot glass
  case named "listitem" ["lin"] (widgets first) of
    Nothing -> pure (Left "the rail does not list the channel")
    Just row -> do
      _ <- press glass row
      threadDelay 3_000_000
      act =<< snapshot glass

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

-- | A body carrying a remote image and a link, neither of which is followed.
aRemoteImageIsNeverFetched :: Door -> Ground -> IO (Either String ())
aRemoteImageIsNeverFetched door ground = withListener BlackHole $ \listener ->
  staged door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> do
      let host = Text.pack (locatorOf listener)
      bobSays door ground ("![probe](" <> host <> "probe.png) see [this](" <> host <> "link)")
      opened glass $ \shown -> do
        let drawn = widgets shown
            altShown = any (\w -> widgetRole w == "text" && "probe" `Text.isInfixOf` widgetName w) drawn
            imageDrawn = any (\w -> widgetRole w == "image" && "probe" `Text.isInfixOf` widgetName w) drawn
        threadDelay 2_000_000
        reached <- connections listener
        pure $
          if not altShown
            then Left ("the body was not rendered at all" <> seen shown)
            else
              if imageDrawn
                then Left "the remote image was drawn as an image rather than as its alt text"
                else
                  if reached /= 0
                    then Left ("the window opened " <> show reached <> " connection(s) to a host the peer named")
                    else noErrorEvent shown

-- | A link — `http:`, `javascript:` or `file:` — has nothing bound to press,
-- and pressing it anyway changes nothing.
aLinkCannotBePressed :: Door -> Ground -> IO (Either String ())
aLinkCannotBePressed door ground = withListener BlackHole $ \listener ->
  staged door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> do
      let host = Text.pack (locatorOf listener)
      bobSays door ground ("[this](" <> host <> "link) and [that](javascript:alert(1)) and [file](file:///C:/Windows/win.ini)")
      opened glass $ \shown -> do
        let links = filter ((== "link") . widgetRole) (widgets shown)
            pressable = filter (elem "press" . widgetActions) links
        forM_ links (press glass)
        threadDelay 2_000_000
        reached <- connections listener
        after <- snapshot glass
        pure $
          if length links < 3
            then Left ("expected three links drawn, found " <> show (length links) <> seen shown)
            else
              if not (null pressable)
                then Left ("a link can be pressed: " <> show (map widgetName pressable))
                else
                  if reached /= 0
                    then Left ("pressing a link opened " <> show reached <> " connection(s)")
                    else noErrorEvent after

-- | Terminal bytes from a peer are drawn as hexadecimal, never as bytes.
controlBytesAreShownAsHex :: Door -> Ground -> IO (Either String ())
controlBytesAreShownAsHex door ground =
  staged door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> do
      let bytes = "\x1b]52;c;aGVsbG8=\x07 clear \x1b[2J\r" :: ByteString
      spoke <- Door.typed door ["--root", siteOf ground Bob, "--json", "send", "--to", "-"] (Just ("me\n" <> bytes))
      case typedStatus spoke of
        ExitFailure code -> pure (Left ("the bytes were refused with exit code " <> show code))
        ExitSuccess -> opened glass $ \shown -> do
          let raw = "\x1b" `Text.isInfixOf` shown || "\r" `Text.isInfixOf` Text.replace "\r\n" "\n" shown
              hex = any (\w -> widgetRole w == "text" && "1b5d35323b633b" `Text.isInfixOf` widgetName w) (widgets shown)
          pure $
            if raw
              then Left "a control byte from the peer reached the widget tree"
              else
                if not hex
                  then Left ("the bytes were not shown as hexadecimal" <> seen shown)
                  else noErrorEvent shown

-- | Drives the invite sheet to a minted invitation; answers whether the line
-- was shown, and the copy button when there is one.
minting :: Glass -> Ground -> IO (Either String Widget)
minting glass ground = do
  first <- snapshot glass
  case named "button" ["新邀请", "New invitation"] (widgets first) of
    Nothing -> pure (Left "the rail has no invite button")
    Just new -> do
      _ <- press glass new
      threadDelay 500_000
      second <- snapshot glass
      let sheet = widgets second
      case (named "textbox" ["名字", "Name"] sheet, named "textbox" ["Waypoint"] sheet, named "button" ["生成", "Mint"] sheet) of
        (Just name, Just host, Just mint) -> do
          _ <- setText glass name "jie"
          _ <- setText glass host (waypoint ground)
          _ <- press glass mint
          threadDelay 2_000_000
          third <- widgets <$> snapshot glass
          pure $
            if not (any (("kusanagi2:" `Text.isPrefixOf`) . widgetName) third)
              then Left "the window did not show the invitation it minted"
              else maybe (Left "the invitation has no copy button") Right (named "button" ["复制邀请", "Copy the invitation"] third)
        _ -> pure (Left ("the invite sheet is missing a field" <> seen second))

-- | After a session, the disk outside the site holds nothing of the peer.
--
-- The site itself (`LOCALAPPDATA/kusanagi`) is the CLI's, and H5 answers for
-- its bytes. Everything else the window or its runtime may write is listed
-- here: two preferences of ours, the runtime's window geometry, and a
-- runtime event log only when the window was built with tracing on — the
-- shipped build is not (`-Dtrace=off`). A file not on the list is a finding.
theDiskHoldsNoPeer :: Door -> Ground -> IO (Either String ())
theDiskHoldsNoPeer door ground =
  staged door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> do
      bobSays door ground "pineapple-on-pizza-7731"
      driven <- opened glass $ \_ -> minting glass ground
      files <- filesOutsideTheSite glass
      judged <- forM files $ \path -> do
        bytes <- ByteString.readFile path
        let needles = ["pineapple-on-pizza-7731", "kusanagi2:", Text.encodeUtf8 (Text.pack (waypoint ground))]
            leaked = filter (`ByteString.isInfixOf` bytes) needles
            known = takeFileName path `elem` ["kusanagi-glass.language", "kusanagi-glass.font", "windows.zon", "native-sdk.jsonl", "last-panic.txt", "glass-out.log", "glass-err.log"]
        pure $
          if not known
            then Left ("the window wrote a file nobody listed: " <> path)
            else
              if null leaked
                then Right ()
                else Left (path <> " holds " <> show leaked)
      pure (driven >> sequence_ judged)

-- | Nothing reaches the clipboard until a person presses copy, and then the
-- window says what the clipboard is.
theClipboardWaitsForAHand :: Door -> Ground -> IO (Either String ())
theClipboardWaitsForAHand door ground =
  staged door ground >>= \case
    Left reason -> pure (Left reason)
    Right glass -> do
      let sentinel = "sentinel-" <> show (length (waypoint ground))
      _ <- powershell ("Set-Clipboard -Value '" <> sentinel <> "'")
      running glass $
        minting glass ground >>= \case
          Left reason -> pure (Left reason)
          Right copy -> do
            untouched <- powershell "Get-Clipboard -Raw"
            if Text.strip (Text.pack untouched) /= Text.pack sentinel
              then pure (Left ("the clipboard changed before anybody pressed copy: " <> take 24 untouched))
              else do
                _ <- press glass copy
                threadDelay 800_000
                copied <- powershell "Get-Clipboard -Raw"
                after <- snapshot glass
                let warned = any (\w -> widgetRole w == "text" && ("剪贴板" `Text.isInfixOf` widgetName w || "clipboard" `Text.isInfixOf` widgetName w)) (widgets after)
                pure $
                  if not ("kusanagi2:" `Text.isPrefixOf` Text.strip (Text.pack copied))
                    then Left "pressing copy did not put the invitation on the clipboard"
                    else
                      if not warned
                        then Left "the window copied without saying what the clipboard is"
                        else noErrorEvent after

powershell :: String -> IO String
powershell command = do
  (_, out, _) <- capture Nothing "powershell" ["-NoProfile", "-Command", command]
  pure (lenient out)

-- | Every file under the throwaway home and app-data directories, except the
-- site the CLI keeps under `LOCALAPPDATA/kusanagi`.
filesOutsideTheSite :: Glass -> IO [FilePath]
filesOutsideTheSite glass = do
  fromHome <- walk (glassHome glass)
  fromAppData <- walk (glassAppData glass)
  pure (filter (not . underSite) (fromHome <> fromAppData))
  where
    underSite path = take (length (glassSite glass)) path == glassSite glass
    walk directory = do
      there <- doesDirectoryExist directory
      if not there
        then pure []
        else do
          entries <- listDirectory directory
          fmap concat . forM entries $ \entry -> do
            let path = directory </> entry
            isDirectory <- doesDirectoryExist path
            if isDirectory then walk path else pure [path]
