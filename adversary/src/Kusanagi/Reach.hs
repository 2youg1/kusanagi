-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE OverloadedStrings #-}

-- | What the client does when the network is not what it was told.
--
-- Every property here is about failing closed. A host that never answers
-- must cost a bounded amount of time and not a hung agent; a host that
-- answers nonsense must produce a coded refusal and never a crash; a host
-- that says "go there instead" must not be obeyed, because "there" is where
-- the client's address would be learned; and a proxy, once named, must be the
-- only thing the client ever connects to — including when the proxy is down,
-- which is the moment a client that fell back to a direct connection would
-- hand its address to the host it was hiding from.
module Kusanagi.Reach
  ( aBlackHoleIsRefusedInBoundedTime
  , garbageIsRefusedNotCrashed
  , aRedirectIsNeverFollowed
  , theHostIsNeverReachedDirectlyWithAProxy
  , aDeadProxyFailsClosed
  , aRequiredProxyThatIsMissingFailsClosed
  , theRequestHeadNamesNothing
  , aLocatorNeverNamesANetworkPath
  , nothingThatNamesThisMachineLeavesIt
  , noVerbConnectsMoreThanItMust
  ) where

import Control.Monad (forM)
import Data.ByteString (ByteString)
import Data.ByteString qualified as ByteString
import Data.ByteString.Char8 qualified as Char8
import Data.Char (toLower)
import Data.List (nub, sort)
import GHC.Clock (getMonotonicTime)
import System.Environment (getEnvironment, lookupEnv)
import System.Exit (ExitCode (..))
import System.Info (arch, os)

import Data.Text qualified as Text
import Data.Text.Encoding qualified as Text

import Kusanagi.Answer (Answer (..), ChannelName (..), Code (..), Complaint (..), Invitation (..), Outcome (..), decodeComplaint, decodeOutcome)
import Kusanagi.Door (Door)
import Kusanagi.Door qualified as Door
import Kusanagi.Ground (Ground, Site (..), siteOf, stored, waypoint)
import Kusanagi.Listener

-- | The one request the door makes first: an invitation writes an offer.
inviting :: Door -> FilePath -> Maybe [(String, String)] -> String -> IO Door.Typed
inviting door site surroundings locator =
  Door.typedWith
    door
    surroundings
    ["--root", site, "--json", "invite", "--name", "-", "--waypoint", locator, "--for", "3600", "--can", "send,read"]
    (Just "reaching-out\n")

-- | Accepting the connection and never answering costs a bounded time.
aBlackHoleIsRefusedInBoundedTime :: Door -> Ground -> IO (Either String ())
aBlackHoleIsRefusedInBoundedTime door ground =
  withListener BlackHole $ \hole -> do
    started <- getMonotonicTime
    answer <- inviting door (siteOf ground Alice) Nothing (locatorOf hole)
    finished <- getMonotonicTime
    arrived <- connections hole
    pure $ do
      if arrived >= 1 then Right () else Left "the client never connected to the black hole"
      refusedWithACode "a host that never answers" answer
      if finished - started < 90
        then Right ()
        else Left ("a host that never answers held the verb for " <> show (finished - started) <> " seconds")

-- | Answers that are not a box's are refused, and the process still leaves by
-- one of the two doors.
garbageIsRefusedNotCrashed :: Door -> Ground -> IO (Either String ())
garbageIsRefusedNotCrashed door ground = do
  findings <- forM scripts $ \(what, bytes) ->
    withListener (Answer bytes) $ \liar -> do
      answer <- inviting door (siteOf ground Alice) Nothing (locatorOf liar)
      pure (oneOfTwoShapes what answer)
  pure (sequence_ findings)
  where
    scripts =
      [ ("a body far shorter than its declared length", "HTTP/1.1 200 OK\r\nContent-Length: 999999\r\n\r\nxx")
      , ("bytes that are not HTTP", "\xff\xfe\x00\x01 this is not a protocol \x00\x00")
      , ("a status line and nothing else", "HTTP/1.1 200 OK\r\n\r\n")
      , ("a 200 with a tiny body where a drop should be", "HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc")
      , ("a chunked body that never ends", "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n")
      , ("a header that never ends", Char8.replicate 20000 'x')
      , ("a 500", "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
      ]

-- | A host that answers "go there instead" is refused, and "there" never
-- hears from the client.
aRedirectIsNeverFollowed :: Door -> Ground -> IO (Either String ())
aRedirectIsNeverFollowed door ground =
  withListener (Answer notFound) $ \there ->
    withListener (Redirect (locatorOf there)) $ \here -> do
      answer <- inviting door (siteOf ground Alice) Nothing (locatorOf here)
      followed <- connections there
      pure $ do
        refusedWithACode "a redirecting host" answer
        if followed == 0 then Right () else Left ("the client followed a redirect " <> show followed <> " time(s)")

-- | With a proxy named, the host is never connected to directly.
theHostIsNeverReachedDirectlyWithAProxy :: Door -> Ground -> IO (Either String ())
theHostIsNeverReachedDirectlyWithAProxy door ground =
  withListener (Answer notFound) $ \host ->
    withListener (Answer "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n") $ \proxy -> do
      surroundings <- withProxy ("http://127.0.0.1:" <> show (portOf proxy))
      answer <- inviting door (siteOf ground Alice) (Just surroundings) (locatorOf host)
      direct <- connections host
      viaProxy <- connections proxy
      pure $ do
        if viaProxy >= 1 then Right () else Left "the proxy was named and never connected to"
        if direct == 0 then Right () else Left ("the host was reached directly " <> show direct <> " time(s) with a proxy named")
        refusedWithACode "a proxy that refuses to connect" answer

-- | A site that recorded "never without a proxy" refuses when the variable is
-- gone — a new shell, a scheduler task — rather than going direct.
aRequiredProxyThatIsMissingFailsClosed :: Door -> Ground -> IO (Either String ())
aRequiredProxyThatIsMissingFailsClosed door ground =
  withListener (Answer notFound) $ \host -> do
    inherited <- getEnvironment
    let without = [(k, v) | (k, v) <- inherited, k /= "KUSANAGI_PROXY"]
    recorded <- Door.typedWith door (Just without) ["--root", siteOf ground Alice, "--json", "proxy", "--require"] Nothing
    answer <- inviting door (siteOf ground Alice) (Just without) (locatorOf host)
    direct <- connections host
    pure $ do
      case Door.typedStatus recorded of
        ExitSuccess -> Right ()
        other -> Left ("recording the requirement failed: " <> show other)
      if direct == 0 then Right () else Left ("the host was reached directly " <> show direct <> " time(s) with a proxy required and none set")
      case decodeComplaint (Door.typedErr answer) of
        Right (Complaint (Code "kusanagi.proxy_required") _ _) -> Right ()
        Right (Complaint (Code code) _ _) -> Left ("refused with `" <> Text.unpack code <> "` rather than `kusanagi.proxy_required`")
        Left reason -> Left ("refused in a shape this adversary cannot read: " <> reason)

-- | A proxy that is down is a refusal, not a direct connection.
aDeadProxyFailsClosed :: Door -> Ground -> IO (Either String ())
aDeadProxyFailsClosed door ground =
  withListener (Answer notFound) $ \host -> do
    findings <- forM ["socks5://127.0.0.1:1", "http://127.0.0.1:1", "socks5h://127.0.0.1:1"] $ \dead -> do
      surroundings <- withProxy dead
      answer <- inviting door (siteOf ground Alice) (Just surroundings) (locatorOf host)
      direct <- connections host
      pure $ do
        if direct == 0 then Right () else Left ("with " <> dead <> " down, the host was reached directly")
        refusedWithACode ("a dead proxy at " <> dead) answer
    pure (sequence_ findings)

-- | Every request head carries only the headers ordinary traffic carries, no
-- user agent, and nothing that names this project.
theRequestHeadNamesNothing :: Door -> Ground -> IO (Either String ())
theRequestHeadNamesNothing door ground =
  withListener (Answer notFound) $ \host -> do
    _ <- inviting door (siteOf ground Alice) Nothing (locatorOf host)
    seen <- heads host
    let names = nub (sort (concatMap headerNames seen))
        strange = [name | name <- names, name `notElem` ordinary]
        telling = [h | h <- seen, "kusanagi" `Char8.isInfixOf` Char8.map toLower h]
    pure $ do
      if null seen then Left "no request head arrived" else Right ()
      case strange of
        [] -> Right ()
        _ -> Left ("a request carried a header ordinary traffic does not: " <> show strange)
      case telling of
        [] -> Right ()
        (h : _) -> Left ("a request names the project: " <> show h)
  where
    ordinary = ["accept", "cache-control", "content-length", "content-type", "host", "if-none-match"]
    -- Only the head before the blank line: the body is 131 072 sealed bytes
    -- and any of them can look like a header line.
    headerNames request =
      [ Char8.unpack (Char8.map toLower name)
      | line <- drop 1 (Char8.lines head')
      , let (name, rest) = Char8.break (== ':') line
      , not (Char8.null rest)
      ]
      where
        head' = fst (ByteString.breakSubstring "\r\n\r\n" request)

-- | Nothing that names this machine, this account, this build or this moment
-- is in any byte that leaves it: not in a request head, not in an object the
-- host stores, not in the invitation line a person carries.
nothingThatNamesThisMachineLeavesIt :: Door -> Ground -> IO (Either String ())
nothingThatNamesThisMachineLeavesIt door ground =
  withListener (Answer notFound) $ \host -> do
    let alice = siteOf ground Alice
    surroundings <- getEnvironment
    _ <- inviting door alice Nothing (locatorOf host)
    minted <- Door.ask door alice (Door.Invite (ChannelName "carried-by-hand") (waypoint ground) Door.Forever Door.both)
    line <- case minted of
      Accepted (Invited _ (Invitation carried) _) -> pure (Text.encodeUtf8 carried)
      other -> fail ("the invitation was refused: " <> show other)
    _ <- Door.ask door (siteOf ground Bob) (Door.Join (Invitation (Text.decodeUtf8 line)) (ChannelName "carried-by-hand"))
    seen <- heads host
    held <- stored ground
    version <- Door.typed door ["--version"] Nothing
    machine <- mapM lookupEnv ["COMPUTERNAME", "USERNAME", "USERPROFILE", "HOSTNAME", "USER", "HOME", "LOGNAME"]
    let needles =
          [Char8.pack value | Just value <- machine, length value >= 4]
            <> [Char8.pack (siteOf ground Alice), Char8.pack os, Char8.pack arch, "windows", "rustc", "cargo"]
            <> [Char8.strip (Door.typedOut version)]
            <> [Char8.pack value | ("PROCESSOR_IDENTIFIER", value) <- surroundings]
        outgoing = [("a request head", h) | h <- seen] <> [("a host object", b) | (_, b) <- held] <> [("the invitation line", line)]
        lowered = Char8.map toLower
    pure $ case [(needle, place) | needle <- needles, (place, bytes) <- outgoing, lowered needle `Char8.isInfixOf` lowered bytes] of
      [] -> Right ()
      ((needle, place) : _) -> Left (place <> " carries " <> show needle)

-- | Verbs that name no host make no connection, and the one that does makes
-- the number its protocol needs and not one more. The proxy is the oracle:
-- named, it is the only place a connection can go.
noVerbConnectsMoreThanItMust :: Door -> Ground -> IO (Either String ())
noVerbConnectsMoreThanItMust door ground =
  withListener (Answer "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n") $ \proxy -> do
    surroundings <- withProxy ("http://127.0.0.1:" <> show (portOf proxy))
    let alice = siteOf ground Alice
        quietly arguments = Door.typedWith door (Just surroundings) (["--root", alice, "--json"] <> arguments) Nothing
    _ <- quietly ["id"]
    _ <- quietly ["channels"]
    _ <- quietly ["export"]
    _ <- quietly ["--version"]
    silent <- connections proxy
    _ <- inviting door alice (Just surroundings) "http://127.0.0.1:9/"
    speaking <- connections proxy
    pure $ do
      if silent == 0 then Right () else Left ("verbs that name no host made " <> show silent <> " connection(s)")
      if speaking - silent == 1 then Right () else Left ("one refused request made " <> show (speaking - silent) <> " connection(s)")

-- | A locator that names a network path is refused at both ends of an
-- invitation with one code, and that code is not a network failure's: the
-- refusal is a decision about the string, made before anything is connected to.
--
-- A UNC path is a network connection the operating system makes on this
-- program's behalf, outside any proxy it was told to use, to a machine the
-- inviter chose — and on Windows that connection authenticates. A dead drop
-- on a file share is still possible: the person mounts it, and the program
-- sees a drive letter.
aLocatorNeverNamesANetworkPath :: Door -> Ground -> IO (Either String ())
aLocatorNeverNamesANetworkPath door ground = do
  let alice = siteOf ground Alice
      minting locator = Door.ask door alice (Door.Invite (ChannelName (Text.pack ("via-" <> filter (`elem` ['a' .. 'z']) locator))) locator Door.Forever Door.both)
  stranger <- minting "http://127.0.0.1:1/"
  unc <- mapM minting ["\\\\127.0.0.1\\nothing\\drops", "//127.0.0.1/nothing/drops", "\\\\?\\UNC\\127.0.0.1\\nothing"]
  genuine <- Door.ask door alice (Door.Invite (ChannelName "genuine") (waypoint ground) Door.Forever Door.both)
  joined <- case genuine of
    Accepted (Invited _ (Invitation line) _) ->
      let secret = Text.take 132 (Text.drop 1 (Text.dropWhile (/= ':') line))
          forged = Invitation ("kusanagi2:" <> secret <> hexOfString "\\\\127.0.0.1\\nothing\\drops")
       in Just <$> Door.ask door (siteOf ground Bob) (Door.Join forged (ChannelName "forged"))
    _ -> pure Nothing
  pure $ do
    dead <- case stranger of
      Refused complaint -> Right (complaintCode complaint)
      Accepted outcome -> Left ("a dead host was accepted: " <> show outcome)
    codes <- sequence
      [ case answer of
          Refused complaint | complaintCode complaint /= dead -> Right (complaintCode complaint)
          Refused complaint -> Left ("a UNC locator was refused with " <> show (complaintCode complaint) <> ", the code a dead host gets: it was connected to")
          Accepted outcome -> Left ("a UNC locator was accepted at invite: " <> show outcome)
      | answer <- unc
      ]
    atJoin <- case joined of
      Just (Refused complaint) | complaintCode complaint /= dead -> Right (complaintCode complaint)
      Just (Refused complaint) -> Left ("an invitation carrying a UNC locator was refused at join with " <> show (complaintCode complaint) <> ", the code a dead host gets: it was connected to")
      Just (Accepted outcome) -> Left ("an invitation carrying a UNC locator was accepted at join: " <> show outcome)
      Nothing -> Left ("no genuine invitation to forge from: " <> show genuine)
    case filter (/= atJoin) codes of
      [] -> Right ()
      other -> Left ("a network path is refused with " <> show other <> " at invite and " <> show atJoin <> " at join")
  where
    hexOfString :: String -> Text.Text
    hexOfString = Text.pack . concatMap (\c -> let n = fromEnum c in [digit (n `div` 16), digit (n `mod` 16)])
    digit d = "0123456789abcdef" !! d

notFound :: ByteString
notFound = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"

-- | This process's environment with the proxy named, so the child is told
-- everything else it needs to start.
withProxy :: String -> IO [(String, String)]
withProxy proxy = do
  inherited <- getEnvironment
  pure (("KUSANAGI_PROXY", proxy) : [(k, v) | (k, v) <- inherited, k /= "KUSANAGI_PROXY"])

refusedWithACode :: String -> Door.Typed -> Either String ()
refusedWithACode what answer =
  case Door.typedStatus answer of
    ExitFailure 1 -> case decodeComplaint (Door.typedErr answer) of
      Right (Complaint (Code code) _ _) | code /= "" -> Right ()
      Right _ -> Left (what <> " was refused without a code")
      Left reason -> Left (what <> " was refused in a shape this adversary cannot read: " <> reason)
    ExitSuccess -> Left (what <> " was accepted: " <> show (Char8.take 200 (Door.typedOut answer)))
    ExitFailure other -> Left (what <> " made the process exit with " <> show other <> ": " <> show (Char8.take 200 (Door.typedErr answer)))

oneOfTwoShapes :: String -> Door.Typed -> Either String ()
oneOfTwoShapes what answer =
  case Door.typedStatus answer of
    ExitSuccess -> either (\reason -> Left (what <> " was accepted unreadably: " <> reason)) (const (Right ())) (decodeOutcome (Door.typedOut answer))
    ExitFailure 1 -> refusedWithACode what answer
    ExitFailure other -> Left (what <> " made the process exit with " <> show other <> ": " <> show (Char8.take 200 (Door.typedErr answer)))
