/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Model

/-!
# What this adversary delivers: a Rust test

A counterexample that stays in the adversary is knowledge this repository does
not have. Rendering it as a test beside the code it accuses moves the knowledge
into the language that ships, and leaves nothing here that could grow into a
second authority.

The output has to survive `cargo fmt --check`, so this module agrees with
rustfmt rather than merely producing valid Rust: a method chain keeps its
receiver on the opening line only when that receiver is no wider than one
indent, which is why `bob` and `alice` come out shaped differently.

Building a runnable trace out of a list of actions, and asking whether every
step of a trace is one the model admits at that point, are
`Kusanagi.Dynamic.sequenced` and `Kusanagi.Dynamic.coherent`. The engine owns
both, and one rule keeps one authority, so this module renders and nothing else.
-/

namespace Kusanagi.Regression

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Dynamic
open Kusanagi.Ground
open Kusanagi.Model

/-- The lines of a rendered file, each closed by a newline. -/
private def unlines (lines : List String) : String :=
  lines.foldl (fun gathered line => gathered ++ line ++ "\n") ""

private def quoted (text : String) : String :=
  let escaped : Char → String
    | '"' => "\\\""
    | '\\' => "\\\\"
    | character => character.toString
  "\"" ++ (text.toList.map escaped).foldl (· ++ ·) "" ++ "\""

private def spelled (channel : ChannelName) : String := channel.said

private def endpoint (site : Site) : String := site.named

private def numbered (bound : Var) : String := toString bound.step

private def ticket (bound : Var) : String := "invitation" ++ numbered bound

private def heard (bound : Var) : String := "heard" ++ numbered bound

private def complaint (bound : Var) : String := "refused" ++ numbered bound

private def whose : Action → Site
  | .invite site _ _ _ => site
  | .join site _ _ => site
  | .send site _ _ => site
  | .read site _ => site
  | .revoke site _ => site

private def verb : Action → String
  | .invite .. => "Invite"
  | .join .. => "Join"
  | .send .. => "Send"
  | .read .. => "Read"
  | .revoke .. => "Revoke"

/--
How a channel is opened, on a trace that never varies it.

The adversary opens on-demand channels that keep their history, because that is
what every property here is about. The field is spelled out rather than left off
so that the rendered test says which of the four combinations it is exercising;
a default that changed under it would be a silent change to what these traces
mean.
-/
private def habit : String := "habit: kusanagi::Habit::default(),"

private def permission (abilities : Abilities) : String :=
  match abilities.maySend, abilities.mayRead with
  | true, true => "Abilities::ALL"
  | true, false => "Abilities::NONE.with(Ability::Send)"
  | false, true => "Abilities::NONE.with(Ability::Read)"
  | false, false => "Abilities::NONE"

private def fields : Action → List String
  | .invite _ channel lifetime abilities =>
    [ "name: " ++ quoted (spelled channel) ++ ".to_owned(),"
    , "waypoint: host.clone(),"
    , "lifetime: " ++ toString lifetime.seconds ++ ","
    , "abilities: " ++ permission abilities ++ ","
    , habit ]
  | .join _ held channel =>
    [ "invite: " ++ ticket held ++ ".clone(),"
    , "name: " ++ quoted (spelled channel) ++ ".to_owned(),"
    , habit ]
  -- A payload is bytes on the Rust side, so the literal is a byte string. The
  -- adversary only ever sends words, and a word is its own ASCII.
  | .send _ channel text =>
    [ "name: " ++ quoted (spelled channel) ++ ".to_owned(),"
    , "payload: b" ++ quoted text ++ ".to_vec()," ]
  -- The trace reads what the other endpoint wrote, which is what `Whose::Peer`
  -- spells now that an endpoint can also read its own stream back.
  | .read _ channel =>
    [ "name: " ++ quoted (spelled channel) ++ ".to_owned(),"
    , "after: None,"
    , "whose: Whose::Peer," ]
  | .revoke _ channel => ["name: " ++ quoted (spelled channel) ++ ".to_owned(),"]

/--
Whether rustfmt keeps a receiver on the line that opens the chain.

The threshold is one indent wide, which is why this depends on the name and not
on the action.
-/
private def attaches (site : Site) : Bool := (endpoint site).length ≤ 4

/-- One call to `run`, laid out the way rustfmt lays a method chain out. -/
private def call (column : Nat) (lead : String) (site : Site) (act : Action)
    (ending : String) : List String :=
  let margin := "".pushn ' ' column
  if attaches site then
    [margin ++ lead ++ endpoint site ++ ".run(&Request::" ++ verb act ++ " {"]
      ++ (fields act).map (fun field => margin ++ "    " ++ field)
      ++ [margin ++ "})", margin ++ ending]
  else
    [ margin ++ lead ++ endpoint site
    , margin ++ "    .run(&Request::" ++ verb act ++ " {" ]
      ++ (fields act).map (fun field => margin ++ "        " ++ field)
      ++ [margin ++ "    })", margin ++ "    " ++ ending]

private def heights (bound : Var) : List String → List String
  | [] => ["    assert!(" ++ heard bound ++ "[\"height\"].is_null());"]
  | said =>
    ("    assert_eq!(" ++ heard bound ++ "[\"height\"], " ++ toString (said.length - 1) ++ ");")
      :: said.zipIdx.map fun (text, index) =>
        "    assert_eq!(" ++ heard bound ++ "[\"segments\"][" ++ toString index
          ++ "][\"text\"], " ++ quoted text ++ ");"

private def succeeded (world : World) (bound : Var) : Action → List String
  | act@(.invite site ..) =>
    ["    let " ++ ticket bound ++ " = json("]
      ++ call 8 "&" site act ".expect(\"the invitation was refused\"),"
      ++ [ "    )[\"invite\"]"
         , "        .as_str()"
         , "        .unwrap()"
         , "        .to_owned();" ]
  | act@(.join site ..) => call 4 "" site act ".expect(\"the invitation was not accepted\");"
  | act@(.send site ..) => call 4 "" site act ".expect(\"the segment was refused\");"
  | act@(.read site channel) =>
    ["    let " ++ heard bound ++ " = json("]
      ++ call 8 "&" site act ".expect(\"the stream was refused\"),"
      ++ ["    );"]
      ++ heights bound (expected world ⟨site, channel⟩)
  | act@(.revoke site _) => call 4 "" site act ".expect(\"the peer could not be cut off\");"

/--
A refusal, in two statements rather than one.

Putting the call inside `assert_eq!` would leave its shape to whatever rustfmt
does inside a macro; taking the error out first keeps the formatting predictable
and the assertion readable.
-/
private def refused (world : World) (bound : Var) (act : Action) : List String :=
  let owed := match refusal world act with
    | some code => code.stable
    | none => "the model owes no code here, which is a bug in the adversary"
  call 4 ("let " ++ complaint bound ++ " = ") (whose act) act ".unwrap_err();"
    ++ ["    assert_eq!(" ++ complaint bound ++ ".code(), " ++ quoted owed ++ ");"]

private def rendered (world : World) (bound : Var) (act : Action) : Polarity → List String
  | .positive => succeeded world bound act
  | .negative => refused world bound act

private def attending (actions : Actions Action) : List Site :=
  actions.foldl (fun seen taken =>
    let acting := whose taken.action
    if seen.contains acting then seen else seen ++ [acting]) []

/--
Whether any step reads a field out of an outcome, and so needs `json`.

Both a successful read and a successful invitation do: one for the segments, the
other for the line it has to hand over.
-/
private def parsing (actions : Actions Action) : Bool :=
  actions.any fun taken =>
    match taken.action, taken.polarity with
    | .read .., .positive => true
    | .invite .., .positive => true
    | _, _ => false

private def inviting (actions : Actions Action) : Bool :=
  actions.any fun taken => match taken.action with
    | .invite .. => true
    | _ => false

/--
Whether the trace reads anything, in either polarity.

A read is the only action that names `Whose`, so this is what decides whether
the emitted file imports it.
-/
private def reading (actions : Actions Action) : Bool :=
  actions.any fun taken => match taken.action with
    | .read .. => true
    | _ => false

/-- Whether any invitation grants one ability but not the other. -/
private def partly (actions : Actions Action) : Bool :=
  actions.any fun taken => match taken.action with
    | .invite _ _ _ abilities => abilities.maySend != abilities.mayRead
    | _ => false

/--
Exactly the imports the emitted test uses.

Not one more: the Rust gate refuses an unused import, so a renderer that always
emitted the same header would produce a file nobody could commit.
-/
private def imports (actions : Actions Action) : List String :=
  -- Braces around a single name are what rustfmt takes away again.
  let taken (from' : String) : List String → List String
    | [only] => ["use " ++ from' ++ "::" ++ only ++ ";"]
    | names => ["use " ++ from' ++ "::{" ++ String.intercalate ", " names ++ "};"]
  -- `Whose` appears only when a read does, for the same reason as every other
  -- name here: an unused import is a file the Rust gate refuses.
  let doors := ["Request"] ++ (if reading actions then ["Whose"] else [])
  let helpers :=
    (if (attending actions).isEmpty then [] else ["Endpoint"])
      ++ (if parsing actions then ["json"] else [])
      ++ ["scratch"]
  let grants :=
    (if inviting actions then ["Abilities"] else [])
      ++ (if partly actions then ["Ability"] else [])
  (if helpers.isEmpty then [] else taken "common" helpers)
    ++ taken "kusanagi" doors
    ++ (if grants.isEmpty then [] else taken "kusanagi_grant" grants)

private def preamble (name : String) (actions : Actions Action) : List String :=
  [ "// This Source Code Form is subject to the terms of the Mozilla Public"
  , "// License, v. 2.0. If a copy of the MPL was not distributed with this"
  , "// file, You can obtain one at https://mozilla.org/MPL/2.0/."
  , "// Copyright (c) 2026 2youg1 and the kusanagi contributors"
  , ""
  , "//! A trace the adversary found, kept here so this repository remembers it."
  , "//!"
  , "//! Written by `adversary/Kusanagi/Regression.lean` and compared against it"
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
  , "" ]
    ++ imports actions
    ++ [ ""
       , "#[test]"
       , "fn " ++ name ++ "() {"
       , "    let ground = scratch(" ++ quoted name ++ ");" ]
    ++ (if inviting actions then ["    let host = ground.join(\"host\").display().to_string();"] else [])
    ++ (attending actions).map (fun site =>
         "    let " ++ endpoint site ++ " = Endpoint::new(ground.join("
           ++ quoted (endpoint site) ++ "));")
    ++ [""]

private def closing : List String :=
  [ "    std::fs::remove_dir_all(&ground).ok();"
  , "}" ]

private def walked (world : World) (index : Nat) : Actions Action → List String
  | [] => []
  | taken :: rest =>
    let after := match taken.polarity with
      | .positive => next world taken.action ⟨index⟩
      | .negative => world
    (rendered world ⟨index⟩ taken.action taken.polarity ++ [""])
      ++ walked after (index + 1) rest

/-- Renders a trace as a Rust integration test. -/
def render (name : String) (actions : Actions Action) : String :=
  unlines (preamble name actions ++ walked (initial : World) 0 actions ++ closing)

end Kusanagi.Regression
