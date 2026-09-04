-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
-- Copyright (c) 2026 2youg1 and the kusanagi contributors

{-# LANGUAGE GADTs #-}
{-# LANGUAGE LambdaCase #-}
{-# LANGUAGE OverloadedStrings #-}

-- | What this adversary delivers: a Rust test.
--
-- A counterexample that stays in Haskell is knowledge this repository does not
-- have. Rendering it as a test beside the code it accuses moves the knowledge
-- into the language that ships, and leaves nothing here that could grow into a
-- second authority.
--
-- The output has to survive `cargo fmt --check`, so this module agrees with
-- rustfmt rather than merely producing valid Rust: a method chain keeps its
-- receiver on the opening line only when that receiver is no wider than one
-- indent, which is why `bob` and `alice` come out shaped differently.
module Kusanagi.Regression
  ( sequenced
  , coherent
  , render
  ) where

import Data.List (nub)
import Data.Text (Text)
import Data.Text qualified as Text
import Test.QuickCheck.StateModel

import Kusanagi.Answer (ChannelName (..), Code (..))
import Kusanagi.Door (Abilities (..), seconds)
import Kusanagi.Ground (Site, named)
import Kusanagi.Model

-- | Builds a runnable trace out of a list of actions.
--
-- The polarity of each step is decided by the model rather than declared here,
-- so a hand-written trace cannot claim that something succeeds where the model
-- says it must fail.
sequenced :: [Any (Action World)] -> Actions World
sequenced = Actions . go initialState 1
  where
    go _ _ [] = []
    go world index (Some act : rest) =
      let positive = precondition world act
          bound = mkVar index
          polar = ActionWithPolarity act (if positive then PosPolarity else NegPolarity)
          next
            | positive = nextState world act bound
            | otherwise = failureNextState world act
       in (bound := polar) : go next (index + 1) rest

-- | Whether every step of a trace is one the model admits at that point.
--
-- A hand-written trace drifts as the model learns more. This turns that drift
-- into a failing test rather than a silently skipped one.
coherent :: Actions World -> Bool
coherent (Actions steps) = go initialState steps
  where
    go _ [] = True
    go world ((bound := ActionWithPolarity act polarity) : rest) =
      admits world act polarity
        && go (if polarity == PosPolarity then nextState world act bound else failureNextState world act) rest
    admits world act PosPolarity = precondition world act
    admits world act NegPolarity = validFailingAction world act && not (precondition world act)

-- | Renders a trace as a Rust integration test.
render :: Text -> Actions World -> Text
render name actions@(Actions steps) =
  Text.unlines (preamble name actions <> walked initialState steps <> closing)
  where
    walked _ [] = []
    walked world ((bound := ActionWithPolarity act polarity) : rest) =
      (step world bound act polarity <> [""])
        <> walked (if polarity == PosPolarity then nextState world act bound else world) rest

preamble :: Text -> Actions World -> [Text]
preamble name actions =
  [ "// This Source Code Form is subject to the terms of the Mozilla Public"
  , "// License, v. 2.0. If a copy of the MPL was not distributed with this"
  , "// file, You can obtain one at https://mozilla.org/MPL/2.0/."
  , "// Copyright (c) 2026 2youg1 and the kusanagi contributors"
  , ""
  , "//! A trace the adversary found, kept here so this repository remembers it."
  , "//!"
  , "//! Written by `adversary/src/Kusanagi/Regression.hs` and compared against it"
  , "//! byte for byte. Change the trace there; changing it here turns the adversary"
  , "//! red, which is exactly what should happen when the two disagree."
  , ""
  , "#![allow("
  , "    clippy::unwrap_used,"
  , "    clippy::expect_used,"
  , "    clippy::panic,"
  , "    clippy::indexing_slicing,"
  , "    reason = \"test code\""
  , ")]"
  , ""
  , "mod common;"
  , ""
  ]
    <> imports actions
    <> [ ""
       , "#[test]"
       , "fn " <> name <> "() {"
       , "    let ground = scratch(" <> quoted name <> ");"
       ]
    <> ["    let host = ground.join(\"host\").display().to_string();" | inviting actions]
    <> [ "    let " <> endpoint site <> " = Endpoint::new(ground.join(" <> quoted (endpoint site) <> "));"
       | site <- attending actions
       ]
    <> [""]

-- | Exactly the imports the emitted test uses.
--
-- Not one more: the Rust gate refuses an unused import, so a renderer that
-- always emitted the same header would produce a file nobody could commit.
imports :: Actions World -> [Text]
imports actions =
  [taken "common" helpers | not (null helpers)]
    <> [taken "kusanagi" doors]
    <> [taken "kusanagi_grant" grants | not (null grants)]
  where
    -- Braces around a single name are what rustfmt takes away again.
    taken from [only] = "use " <> from <> "::" <> only <> ";"
    taken from names = "use " <> from <> "::{" <> Text.intercalate ", " names <> "};"
    -- `Whose` appears only when a read does, for the same reason as every other
    -- name here: an unused import is a file the Rust gate refuses.
    doors = ["Request"] <> ["Whose" | reading actions]
    helpers =
      concat
        [ ["Endpoint" | not (null (attending actions))]
        , ["json" | parsing actions]
        , ["scratch"]
        ]
    grants =
      concat
        [ ["Abilities" | inviting actions]
        , ["Ability" | partly actions]
        ]

closing :: [Text]
closing =
  [ "    std::fs::remove_dir_all(&ground).ok();"
  , "}"
  ]

step :: World -> Var a -> Action World a -> Polarity -> [Text]
step world bound act = \case
  PosPolarity -> succeeded world bound act
  NegPolarity -> refused world bound act

succeeded :: World -> Var a -> Action World a -> [Text]
succeeded world bound act = case act of
  Invite site _ _ _ ->
    ["    let " <> ticket bound <> " = json("]
      <> call 8 "&" site act ".expect(\"the invitation was refused\"),"
      <> [ "    )[\"invite\"]"
         , "        .as_str()"
         , "        .unwrap()"
         , "        .to_owned();"
         ]
  Join site _ _ ->
    call 4 "" site act ".expect(\"the invitation was not accepted\");"
  Send site _ _ ->
    call 4 "" site act ".expect(\"the segment was refused\");"
  Read site name ->
    ["    let " <> heard bound <> " = json("]
      <> call 8 "&" site act ".expect(\"the stream was refused\"),"
      <> ["    );"]
      <> heights bound (expected world (site, name))
  Revoke site _ ->
    call 4 "" site act ".expect(\"the peer could not be cut off\");"

-- | A refusal, in two statements rather than one.
--
-- Putting the call inside `assert_eq!` would leave its shape to whatever
-- rustfmt does inside a macro; taking the error out first keeps the formatting
-- predictable and the assertion readable.
refused :: World -> Var a -> Action World a -> [Text]
refused world bound act =
  call 4 ("let " <> complaint bound <> " = ") (whose act) act ".unwrap_err();"
    <> [ "    assert_eq!("
           <> complaint bound
           <> ".code(), "
           <> quoted (owed (refusal world act))
           <> ");"
       ]
  where
    owed (Just (Code code)) = code
    owed Nothing = "the model owes no code here, which is a bug in the adversary"

-- | One call to `run`, laid out the way rustfmt lays a method chain out.
call :: Int -> Text -> Site -> Action World a -> Text -> [Text]
call column lead site act ending
  | attaches site =
      [margin <> lead <> endpoint site <> ".run(&Request::" <> verb act <> " {"]
        <> [margin <> "    " <> field | field <- fields act]
        <> [margin <> "})", margin <> ending]
  | otherwise =
      [ margin <> lead <> endpoint site
      , margin <> "    .run(&Request::" <> verb act <> " {"
      ]
        <> [margin <> "        " <> field | field <- fields act]
        <> [margin <> "    })", margin <> "    " <> ending]
  where
    margin = Text.replicate column " "

-- | Whether rustfmt keeps a receiver on the line that opens the chain.
--
-- The threshold is one indent wide, which is why this depends on the name and
-- not on the action.
attaches :: Site -> Bool
attaches site = Text.length (endpoint site) <= 4

verb :: Action World a -> Text
verb = \case
  Invite {} -> "Invite"
  Join {} -> "Join"
  Send {} -> "Send"
  Read {} -> "Read"
  Revoke {} -> "Revoke"

-- | How a channel is opened, on a trace that never varies it.
--
-- The adversary opens on-demand channels that keep their history, because that
-- is what every property here is about. The field is spelled out rather than
-- left off so that the rendered test says which of the four combinations it is
-- exercising; a default that changed under it would be a silent change to what
-- these traces mean.
habit :: Text
habit = "habit: kusanagi::Habit::default(),"

fields :: Action World a -> [Text]
fields = \case
  Invite _ name lifetime abilities ->
    [ "name: " <> quoted (spelled name) <> ".to_owned(),"
    , "waypoint: host.clone(),"
    , "lifetime: " <> Text.pack (show (seconds lifetime)) <> ","
    , "abilities: " <> permission abilities <> ","
    , habit
    ]
  Join _ held name ->
    [ "invite: " <> ticket held <> ".clone(),"
    , "name: " <> quoted (spelled name) <> ".to_owned(),"
    , habit
    ]
  -- A payload is bytes on the Rust side, so the literal is a byte string. The
  -- adversary only ever sends words, and a word is its own ASCII.
  Send _ name text ->
    [ "name: " <> quoted (spelled name) <> ".to_owned(),"
    , "payload: b" <> quoted text <> ".to_vec(),"
    ]
  -- The trace reads what the other endpoint wrote, which is what `Whose::Peer`
  -- spells now that an endpoint can also read its own stream back.
  Read _ name ->
    [ "name: " <> quoted (spelled name) <> ".to_owned(),"
    , "after: None,"
    , "whose: Whose::Peer,"
    ]
  Revoke _ name -> ["name: " <> quoted (spelled name) <> ".to_owned(),"]

heights :: Var a -> [Text] -> [Text]
heights bound [] = ["    assert!(" <> heard bound <> "[\"height\"].is_null());"]
heights bound said =
  ("    assert_eq!(" <> heard bound <> "[\"height\"], " <> Text.pack (show (length said - 1)) <> ");")
    : [ "    assert_eq!("
          <> heard bound
          <> "[\"segments\"]["
          <> Text.pack (show index)
          <> "][\"text\"], "
          <> quoted text
          <> ");"
      | (index, text) <- zip [(0 :: Int) ..] said
      ]

whose :: Action World a -> Site
whose = \case
  Invite site _ _ _ -> site
  Join site _ _ -> site
  Send site _ _ -> site
  Read site _ -> site
  Revoke site _ -> site

attending :: Actions World -> [Site]
attending (Actions steps) = nub (map acting steps)
  where
    acting (_ := polar) = whose (polarAction polar)

-- | Whether any step reads a field out of an outcome, and so needs `json`.
--
-- Both a successful read and a successful invitation do: one for the segments,
-- the other for the line it has to hand over.
parsing :: Actions World -> Bool
parsing (Actions steps) = any reported steps
  where
    reported (_ := ActionWithPolarity (Read _ _) PosPolarity) = True
    reported (_ := ActionWithPolarity Invite {} PosPolarity) = True
    reported _ = False

inviting :: Actions World -> Bool
inviting (Actions steps) = any anInvite steps
  where
    anInvite (_ := ActionWithPolarity Invite {} _) = True
    anInvite _ = False

-- | Whether the trace reads anything, in either polarity.
--
-- A read is the only action that names `Whose`, so this is what decides whether
-- the emitted file imports it.
reading :: Actions World -> Bool
reading (Actions steps) = any aRead steps
  where
    aRead (_ := ActionWithPolarity Read {} _) = True
    aRead _ = False

-- | Whether any invitation grants one ability but not the other.
partly :: Actions World -> Bool
partly (Actions steps) = any lopsided steps
  where
    lopsided (_ := ActionWithPolarity (Invite _ _ _ abilities) _) =
      maySend abilities /= mayRead abilities
    lopsided _ = False

permission :: Abilities -> Text
permission abilities = case (maySend abilities, mayRead abilities) of
  (True, True) -> "Abilities::ALL"
  (True, False) -> "Abilities::NONE.with(Ability::Send)"
  (False, True) -> "Abilities::NONE.with(Ability::Read)"
  (False, False) -> "Abilities::NONE"

endpoint :: Site -> Text
endpoint = Text.pack . named

ticket :: Var a -> Text
ticket bound = "invitation" <> numbered bound

heard :: Var a -> Text
heard bound = "heard" <> numbered bound

complaint :: Var a -> Text
complaint bound = "refused" <> numbered bound

numbered :: Var a -> Text
numbered = Text.drop 3 . Text.pack . show

spelled :: ChannelName -> Text
spelled (ChannelName name) = name

quoted :: Text -> Text
quoted text = "\"" <> Text.concatMap escaped text <> "\""
  where
    escaped '"' = "\\\""
    escaped '\\' = "\\\\"
    escaped character = Text.singleton character
