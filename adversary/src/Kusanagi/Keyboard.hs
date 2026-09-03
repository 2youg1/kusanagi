-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}

-- | How a person actually reaches this program, and whether what it says back
-- can be acted on.
--
-- Everything else in this adversary walks verbs. Nobody types a verb: they type
-- a line, and they mistype it — a missed key, two letters swapped, caps lock
-- still on, half an invitation because the paste was cut, a shell that left its
-- quotes behind. An agent does something else again: it pipes bytes it did not
-- choose the shape of.
--
-- The four properties here are relations, never expected outputs. They say that
-- the two doors agree, that advice can be followed, that advice is about what
-- was actually supplied, and that bytes survive the trip.
module Kusanagi.Keyboard
  ( Bench (..)
  , Typing (..)
  , Slip (..)
  , Choice (..)
  , slips
  , prepare
  , typingOf
  , shapeIsAnswerable
  , adviceIsExecutable
  , adviceIsAboutWhatWasGiven
  , bytesSurviveTheTrip
  , commandsIn
  ) where

import Data.ByteString qualified as ByteString
import Data.Char (isUpper)
import Data.Text (Text)
import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text
import System.Exit (ExitCode (..))
import Test.QuickCheck (Arbitrary (..), chooseInt)

import Kusanagi.Answer
  ( Answer (..)
  , Complaint (..)
  , Entry (..)
  , Invitation (..)
  , Outcome (..)
  , decodeComplaint
  , decodeOutcome
  , unCode
  )
import Kusanagi.Door (Door, Typed (..))
import Kusanagi.Door qualified as Door

-- | A world that already exists, described the way a keyboard sees it.
--
-- Not the model's `World`: this one holds strings, because what a person types
-- is a string and the point of this module is to mistype exactly those.
data Bench = Bench
  { benchSite :: FilePath
  , benchOther :: FilePath
  , benchWaypoint :: FilePath
  , benchChannel :: String
  , benchInvitation :: String
  }
  deriving stock (Eq, Show)

-- | One way a hand goes wrong, with a name a counterexample can print.
data Slip = Slip
  { slipName :: String
  , slipHit :: String -> String
  }

instance Show Slip where
  show = slipName

-- | A command line as it was meant, and as it came out.
data Typing = Typing
  { intended :: [String]
  , keyed :: [String]
  , slipped :: Slip
  }
  deriving stock (Show)

-- | The mistakes a keyboard actually makes.
--
-- Each is one physical event, not a category of malformedness: a key missed, two
-- fingers in the wrong order, caps lock, a paste that took half, a shell that
-- kept its quotes, a dash that did not register.
slips :: [Slip]
slips =
  [ Slip "a missed key" (drop 1)
  , Slip "a dropped last key" (\word -> take (length word - 1) word)
  , Slip "two letters swapped" swapped
  , Slip "caps lock" (map upper)
  , Slip "a trailing space" (<> " ")
  , Slip "a leading space" (" " <>)
  , Slip "quotes the shell kept" (\word -> "\"" <> word <> "\"")
  , Slip "half a paste" (\word -> take (max 1 (length word `div` 2)) word)
  , Slip "a dash that did not register" (\case '-' : rest -> rest; word -> word)
  , Slip "a doubled key" (\case letter : rest -> letter : letter : rest; word -> word)
  ]
  where
    swapped (first : second : rest) = second : first : rest
    swapped word = word
    upper letter
      | isUpper letter = letter
      | otherwise = toUpperAscii letter
    toUpperAscii letter
      | letter >= 'a' && letter <= 'z' = toEnum (fromEnum letter - 32)
      | otherwise = letter

-- | Which line, which slip, which token — three numbers and nothing else.
--
-- Generation happens before there is a world to type at: the ground is built in
-- IO, and a generator that needed it could not shrink. So what is generated is
-- the *choice*, and `typingOf` applies it once the bench exists.
data Choice = Choice
  { chosenLine :: Int
  , chosenSlip :: Int
  , chosenToken :: Int
  }
  deriving stock (Eq, Show)

instance Arbitrary Choice where
  arbitrary = Choice <$> index <*> index <*> index
    where
      index = chooseInt (0, 999)
  shrink (Choice line slip token) =
    [Choice line' slip token | line' <- shrink line, line' >= 0]
      <> [Choice line slip' token | slip' <- shrink slip, slip' >= 0]
      <> [Choice line slip token' | token' <- shrink token, token' >= 0]

-- | A command somebody meant to type, with one slip in it.
--
-- The command lines are the ones in `README.md` and `docs/joining.md`, because
-- those are the ones people copy. The slip lands on any token, including the
-- verb and the flags — a person mistypes those as readily as a value.
typingOf :: Bench -> Choice -> Typing
typingOf bench choice =
  Typing
    { intended = line
    , keyed = zipWith (\index word -> if index == at then slipHit slip word else word) [0 ..] line
    , slipped = slip
    }
  where
    available = lines_ bench
    line = available !! (chosenLine choice `mod` length available)
    slip = slips !! (chosenSlip choice `mod` length slips)
    at = chosenToken choice `mod` length line

lines_ :: Bench -> [[String]]
lines_ bench =
  [ ["--root", benchSite bench, "id"]
  , ["--root", benchSite bench, "channels"]
  , ["--root", benchSite bench, "read", "--from", benchChannel bench]
  , ["--root", benchSite bench, "read", "--from", benchChannel bench, "--mine"]
  , ["--root", benchSite bench, "read", "--from", benchChannel bench, "--after", "0"]
  , ["--root", benchSite bench, "send", "--to", benchChannel bench, "a line of text"]
  , ["--root", benchSite bench, "revoke", "--from", benchChannel bench]
  , ["--root", benchSite bench, "forget", "--channel", benchChannel bench]
  , ["--root", benchSite bench, "doctor", benchWaypoint bench]
  -- No invitation on the line: the product takes it on stdin, and this bench
  -- gives it none, so the refusal that follows is the one a person gets when
  -- they forget to pipe.
  , ["--root", benchOther bench, "join", "--name", "someone"]
  , ["--root", benchOther bench, "invite", "--name", "carol", "--waypoint", benchWaypoint bench]
  , ["--root", benchOther bench, "invite", "--name", "carol", "--waypoint", benchWaypoint bench, "--can", "read"]
  ]

-- | A world with two endpoints, one channel and one spent invitation in it.
--
-- Built once per test case rather than once per run, because following advice
-- is allowed to change things — `forget` really forgets — and a property whose
-- earlier cases decide its later ones is not a property.
prepare :: Door -> FilePath -> FilePath -> FilePath -> IO Bench
prepare door alice bob host = do
  minted <- Door.typed door (root alice <> ["invite", "--name", "bob", "--waypoint", host]) Nothing
  invitation <- case decodeOutcome (typedOut minted) of
    Right (Invited _ (Invitation line) _) -> pure (Text.unpack line)
    other -> fail ("the bench could not be built: " <> show other)
  _ <-
    Door.typed
      door
      (root bob <> ["join", "--name", "alice"])
      (Just (Text.encodeUtf8 (Text.pack invitation)))
  _ <- Door.typed door (root alice <> ["send", "--to", "bob", "a first thing"]) Nothing
  _ <- Door.typed door (root alice <> ["read", "--from", "bob"]) Nothing
  pure
    Bench
      { benchSite = alice
      , benchOther = bob
      , benchWaypoint = host
      , benchChannel = "bob"
      , benchInvitation = invitation
      }
  where
    root site = ["--root", site, "--json"]

-- | Both doors say the same thing, and each says one of the two things it may.
--
-- A command either worked — exit 0, and stdout is an outcome — or it did not —
-- exit 1, and stderr is a complaint carrying a stable code and a way out. Any
-- third shape is a hole in the door: an exit code nobody documented, a refusal
-- with nothing machine-readable in it, or a code that is not a code.
shapeIsAnswerable :: Door -> [String] -> IO (Either String Answer)
shapeIsAnswerable door arguments = do
  spoke <- Door.typed door (arguments <> ["--json"]) Nothing
  pure $ case typedStatus spoke of
    ExitSuccess -> case decodeOutcome (typedOut spoke) of
      Right outcome -> Right (Accepted outcome)
      Left reason -> Left ("it succeeded but stdout is not an outcome: " <> reason)
    ExitFailure 1 -> case decodeComplaint (typedErr spoke) of
      Left reason ->
        Left $
          "it refused, but stderr is not a complaint: "
            <> reason
            <> "\n  said: "
            <> show (ByteString.take 300 (typedErr spoke))
      Right complaint
        | Text.null (unCode (complaintCode complaint)) -> Left "the complaint carries an empty code"
        | not (Text.isInfixOf "." (unCode (complaintCode complaint))) ->
            Left ("the code is not namespaced: " <> Text.unpack (unCode (complaintCode complaint)))
        | Text.null (Text.strip (complaintRecover complaint)) ->
            Left ("`" <> Text.unpack (unCode (complaintCode complaint)) <> "` carries no way out")
        | otherwise -> Right (Refused complaint)
    ExitFailure code ->
      Left $
        "it left with exit code "
          <> show code
          <> ", which is neither success nor a refusal this door defines\n  said: "
          <> show (ByteString.take 300 (typedErr spoke))

-- | Every command a refusal names is a command this program admits.
--
-- A concrete one is run: it may fail for any reason except being unreadable —
-- `kusanagi.argument` means the advice itself does not parse, which is advice
-- nobody can take. A template one, marked by `<angle brackets>` or a SHOUTED
-- word, cannot be run literally, so what is checked is that its verb exists.
adviceIsExecutable :: Door -> FilePath -> Complaint -> IO (Either String ())
adviceIsExecutable door site complaint =
  go (commandsIn (complaintRecover complaint))
  where
    go [] = pure (Right ())
    go (command : rest)
      | any placeholder command = verbExists command >>= either (pure . Left) (const (go rest))
      | otherwise = runs command >>= either (pure . Left) (const (go rest))

    placeholder word =
      ("<" `isPrefix` word) || all (\letter -> isUpper letter || letter == '_') (filter (/= '-') word)

    isPrefix small big = Text.isPrefixOf (Text.pack small) (Text.pack big)

    verbExists [] = pure (Left "the recovery names `kusanagi` with no verb after it")
    -- `kusanagi <VERB> --help` is a sentence about the program, not a command
    -- to run: the verb slot is marked as the reader's to fill in, and marking it
    -- is the whole of what this property asks for.
    verbExists (verb : _) | placeholder verb = pure (Right ())
    verbExists (verb : _) = do
      spoke <- Door.typed door [verb, "--help"] Nothing
      pure $ case typedStatus spoke of
        ExitSuccess -> Right ()
        _ -> Left ("the recovery names a verb this program does not have: " <> verb)

    -- Following advice may fail: `kusanagi read --from N` on a channel whose
    -- peer is gone is still the right thing to have been told. What it may not
    -- do is fail to *parse* — that is advice nobody can take — and it may not
    -- leave by an exit this door does not define. Success is not inspected
    -- further, because `--help` is a document rather than an outcome.
    runs command = do
      spoke <- Door.typed door (["--root", site, "--json"] <> command) Nothing
      pure $ case typedStatus spoke of
        ExitSuccess -> Right ()
        ExitFailure 1 -> case decodeComplaint (typedErr spoke) of
          Left reason -> Left ("following the advice produced an unreadable refusal: " <> reason)
          Right followed
            | unCode (complaintCode followed) == "kusanagi.argument" ->
                Left $
                  "the advice does not parse: `kusanagi "
                    <> unwords command
                    <> "` answers "
                    <> Text.unpack (complaintMessage followed)
            | otherwise -> Right ()
        ExitFailure code ->
          Left ("following the advice left with exit code " <> show code)

-- | Advice about an invitation is only given to somebody who supplied one.
--
-- The failure this rules out is specific and was real: mistype a channel name
-- and be told to paste the whole `kusanagi1:` line, which sends a confused
-- person to look for a thing they never had.
--
-- "Supplied one" is judged by the position the caller was standing in, not by
-- whether the text still contains the prefix. This adversary found the reason:
-- it dropped the leading @k@ from an otherwise perfect invitation, and a rule
-- that searched for @kusanagi1:@ concluded that no invitation had been offered —
-- so it called correct advice a defect. A mangled invitation is an invitation
-- supplied, and @join@ is the one verb whose positional argument is one.
adviceIsAboutWhatWasGiven :: [String] -> Complaint -> Either String ()
adviceIsAboutWhatWasGiven arguments complaint
  | mentionsInvitation && not suppliedInvitation =
      Left $
        "the advice is about an invitation, and no invitation was supplied: "
          <> Text.unpack (complaintRecover complaint)
  | otherwise = Right ()
  where
    mentionsInvitation = Text.isInfixOf "kusanagi1:" (complaintRecover complaint)
    suppliedInvitation =
      "join" `elem` arguments || any (Text.isInfixOf "kusanagi1:" . Text.pack) arguments

-- | What an agent pipes in comes back out, byte for byte.
--
-- `text` in the same record is lossy and says so; `payload` is the field a
-- caller parses, and this is the only place its promise is tested against bytes
-- nobody chose by hand.
bytesSurviveTheTrip ::
  Door -> FilePath -> String -> ByteString.ByteString -> IO (Either String ())
bytesSurviveTheTrip door site channel payload = do
  spoke <- Door.typed door ["--root", site, "--json", "send", "--to", channel] (Just payload)
  case typedStatus spoke of
    ExitFailure code ->
      pure (Left ("the payload was refused with exit code " <> show code))
    ExitSuccess -> do
      heard <- Door.typed door ["--root", site, "--json", "read", "--from", channel, "--mine"] Nothing
      pure $ case decodeOutcome (typedOut heard) of
        Left reason -> Left ("reading it back failed: " <> reason)
        Right (Read _ _ _ entries) -> case reverse entries of
          [] -> Left "the segment was accepted and then not there"
          (latest : _)
            | entryPayload latest == wire payload -> Right ()
            | otherwise ->
                Left $
                  "what came back is not what went in:\n  in:  "
                    <> show (wire payload)
                    <> "\n  out: "
                    <> show (entryPayload latest)
        Right other -> Left ("reading it back answered " <> show other)
  where
    wire = Text.concat . map hex . ByteString.unpack
    hex byte =
      let digits = "0123456789abcdef"
          high = fromIntegral byte `div` 16
          low = fromIntegral byte `mod` 16
       in Text.pack [digits !! high, digits !! low]

-- | The commands a recovery line names, as argv, with `kusanagi` dropped.
--
-- Recovery text is written for a person, so a command appears inside it as
-- prose: in backticks, or after a pipe, or at the end of a sentence. What is
-- taken is everything from `kusanagi` up to the next backtick, comma, or end of
-- line, which is how a person reads it too.
commandsIn :: Text -> [[String]]
commandsIn recover =
  [ words (Text.unpack (Text.takeWhile (`notElem` ("`,\n" :: String)) rest))
  | rest <- drop 1 (Text.splitOn "kusanagi " recover)
  ]
