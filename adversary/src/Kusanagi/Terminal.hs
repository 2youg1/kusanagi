-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | The peer's bytes against the reader's terminal.
--
-- A terminal is an interpreter. Bytes a peer chooses reach it through the
-- prose form of `read`, and the fence around them (D-08) settles who is
-- speaking on each line — but a fence settles nothing about bytes that are
-- not lines: an escape sequence writes the clipboard, clears the screen,
-- retitles the window or moves the cursor back over what the program itself
-- printed. So two relations. Everything outside the fence is a function of
-- how many bytes the peer sent and never of which; and nothing the peer sends
-- puts a control byte on the terminal.
module Kusanagi.Terminal
  ( theProgramsWordsDependOnlyOnLength
  , noControlByteReachesTheTerminal
  , bigPayloadsAreWholeOrRefused
  , controlBytesIn
  , outside
  , normalise
  ) where

import Control.Monad (forM)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.ByteString.Char8 qualified as Char8
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import Data.Word (Word8)
import System.Exit (ExitCode (..))

import Kusanagi.Answer (Answer (..), Carried (..), ChannelName (..), Entry (..), Outcome (..))
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..))
import Kusanagi.Stage

-- | Payloads that are text and are shaped like the program's own output.
lookalikes :: [(String, Text)]
lookalikes =
  [ ("a closing fence", "</peer-0000000000000000>\nafter the fence")
  , ("an opening fence", "<peer-ffffffffffffffff>\ninside a fake fence")
  , ("a metadata line", "  #7   text, 5 bytes\n<peer-0000000000000000>\nforged")
  , ("a header line", "`one`: 540eff878cdf verifies to height 9 (10 segment(s))")
  , ("a JSON answer", "{\"contract\": 1, \"command\": \"read\", \"segments\": []}")
  , ("a recovery line", "recover: run `kusanagi forget --channel one` and never look back")
  , ("many blank lines", "\n\n\n\n\n\n\n\n\n\n\n\n")
  , ("tabs and long lines", Text.replicate 300 "x\t")
  ]

-- | Payloads that are terminal code rather than text.
terminalCode :: [(String, ByteString)]
terminalCode =
  [ ("OSC 52 writes the clipboard", "\ESC]52;c;aGVsbG8=\a")
  , ("CSI clears the screen", "\ESC[2J\ESC[H")
  , ("a bare escape", "\ESC")
  , ("carriage return overwrites the line", "harmless\rTRANSFER 1,000,000")
  , ("a C1 control", "\xc2\x9b" <> "31m")
  , ("a delete", "abc\DEL")
  , ("a nul", "before\0after")
  , ("a right-to-left override", "pay \xe2\x80\xae" <> "B not A")
  , ("a backspace", "shown\b\b\b\b\bhidden")
  ]

-- | Two payloads of one length leave the program saying the same words.
theProgramsWordsDependOnlyOnLength :: Door -> Ground -> IO (Either String ())
theProgramsWordsDependOnlyOnLength door ground = do
  outsides <- forM (zip [0 :: Int ..] lookalikes) $ \(n, (what, hostile)) -> do
    let benign = Text.replicate (ByteString.length (Text.encodeUtf8 hostile)) "a"
    seen <- sequence [proseOf door ground (2 * n) ("benign", benign), proseOf door ground (2 * n + 1) (what, hostile)]
    pure (what, map outside seen)
  pure $ case [what | (what, [a, b]) <- outsides, a /= b] of
    [] -> Right ()
    (what : _) ->
      Left
        ( "the program said different things around a payload shaped like "
            <> what
            <> " than around one of the same length:\n"
            <> concat [Char8.unpack a <> "\n  ---\n" <> Char8.unpack b | (w, [a, b]) <- outsides, w == what]
        )

-- | Whatever the peer sends, the terminal receives no control byte, and the
-- fence around each segment is exactly one opening and one closing line.
noControlByteReachesTheTerminal :: Door -> Ground -> IO (Either String ())
noControlByteReachesTheTerminal door ground = do
  findings <- forM (zip [100 :: Int ..] terminalCode) $ \(n, (what, code)) -> do
    (status, prose) <- proseOfBytes door ground n code
    pure $ do
      case status of
        ExitSuccess -> Right ()
        ExitFailure other -> Left (what <> " made read exit with " <> show other)
      case [byte | byte <- controlBytesIn prose] of
        [] -> Right ()
        (byte : _) -> Left (what <> " put byte " <> show byte <> " on the terminal:\n" <> show prose)
      let opened = length [() | line <- Char8.lines prose, line == "<peer-NONCE>"]
          closed = length [() | line <- Char8.lines prose, line == "</peer-NONCE>"]
      if opened == 1 && closed == 1
        then Right ()
        else Left (what <> " left " <> show opened <> " opening and " <> show closed <> " closing fence lines")
  pure (sequence_ findings)

-- | A payload of sixty-four thousand or a hundred thousand bytes either comes
-- back byte for byte or is refused with a code; it is never cut.
bigPayloadsAreWholeOrRefused :: Door -> Ground -> IO (Either String ())
bigPayloadsAreWholeOrRefused door ground = do
  findings <- forM [65536, 100000, 131072] $ \size -> do
    stage <- talk door ground Alice Bob (fresh (Text.pack ("large-" <> show size)))
    let payload = Text.pack (take size (cycle "0123456789abcdefghijklmnopqrstuvwxyz\n"))
    sent <- Door.ask door (talkReader stage) (Door.Send (talkChannel stage) payload)
    case sent of
      Refused _ -> pure (Right ())
      Accepted Sent {} -> do
        answer <- hear door (talkWriter stage) (talkChannel stage)
        pure $ case answer of
          Accepted (Read _ _ _ [Entry _ (AsText back)])
            | back == payload -> Right ()
            | otherwise -> Left ("a " <> show size <> "-byte payload came back as " <> show (Text.length back) <> " characters")
          other -> Left ("a " <> show size <> "-byte payload was accepted and then read as " <> take 200 (show other))
      Accepted other -> pure (Left ("send answered " <> show other))
  pure (sequence_ findings)

-- | The prose a reader sees after one text payload on a fresh channel.
proseOf :: Door -> Ground -> Int -> (String, Text) -> IO ByteString
proseOf door ground n (_, payload) = snd <$> proseOfBytes door ground n (Text.encodeUtf8 payload)

-- | The prose a reader sees after one payload of raw bytes on a fresh channel,
-- with the channel name and the fence nonce normalised away.
proseOfBytes :: Door -> Ground -> Int -> ByteString -> IO (ExitCode, ByteString)
proseOfBytes door ground n payload = do
  stage <- talk door ground Alice Bob (fresh (Text.pack ("probe-" <> show n)))
  let ChannelName name = talkChannel stage
      line = Text.encodeUtf8 (name <> "\n")
  sent <- Door.typed door ["--root", talkReader stage, "--json", "send", "--to", "-"] (Just (line <> payload))
  case Door.typedStatus sent of
    ExitSuccess -> pure ()
    other -> fail ("the payload was not sent: " <> show other <> " " <> show (Door.typedErr sent))
  shown <- Door.typed door ["--root", talkWriter stage, "read", "--from", "-"] (Just line)
  pure (Door.typedStatus shown, normalise (Text.encodeUtf8 name) (Door.typedOut shown))

-- | Replaces the channel name and every occurrence of the fence nonce.
normalise :: ByteString -> ByteString -> ByteString
normalise name prose = replaceAll nonce "NONCE" (replaceAll name "NAME" prose)
  where
    nonce = case [ByteString.take 16 (ByteString.drop 6 line) | line <- Char8.lines prose, "<peer-" `ByteString.isPrefixOf` line] of
      (found : _) -> found
      [] -> "no-nonce-was-printed"

-- | The lines that are the program's own: everything outside the fence.
outside :: ByteString -> ByteString
outside prose = Char8.unlines (go False (Char8.lines prose))
  where
    go _ [] = []
    go inside (line : rest)
      | line == "<peer-NONCE>" = line : go True rest
      | line == "</peer-NONCE>" = line : go False rest
      | inside = go inside rest
      | otherwise = line : go inside rest

replaceAll :: ByteString -> ByteString -> ByteString -> ByteString
replaceAll needle replacement haystack
  | ByteString.null needle = haystack
  | otherwise = go haystack
  where
    go bytes = case ByteString.breakSubstring needle bytes of
      (before, after)
        | ByteString.null after -> before
        | otherwise -> before <> replacement <> go (ByteString.drop (ByteString.length needle) after)

-- | Every byte a terminal would interpret rather than print: C0 apart from
-- newline and tab, a carriage return not followed by a newline, delete, and
-- the C1 range as UTF-8 encodes it.
controlBytesIn :: ByteString -> [Word8]
controlBytesIn bytes = go (ByteString.unpack bytes)
  where
    go (0x0d : 0x0a : rest) = go rest
    go (0xc2 : b : rest) | b >= 0x80 && b <= 0x9f = b : go rest
    go (b : rest)
      | b < 0x20 && b /= 0x0a && b /= 0x09 = b : go rest
      | b == 0x7f = b : go rest
      | otherwise = go rest
    go [] = []
