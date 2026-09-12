/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Std.Data.TreeMap
import Kusanagi.Door
import Kusanagi.Dynamic
import Kusanagi.Ground

/-!
# What has to hold between what an endpoint was told and what it can see

The model remembers only what a person would remember: which channels are open,
who is at the other end, what each side has said, who has been cut off, and
which invitations have been spent. It never predicts an address, a digest, a
signature or a ciphertext — those are rules, they already have an authority in
Rust, and restating one here is how a second authority begins.

What it does assert is which failure a caller sees. A stable code is part of the
door's contract rather than a derived value, so pinning it pins the promise the
product makes to the agent on the other side.
-/

namespace Kusanagi.Model

open Std (TreeMap)
open Kusanagi.Answer
open Kusanagi.Check
open Kusanagi.Door
open Kusanagi.Dynamic
open Kusanagi.Ground

/--
One channel as one endpoint holds it: a site plus the name it uses locally.

Names are local, so the same conversation is a different slot on each side and
the model has to carry the link between them rather than assume one.
-/
structure Slot where
  site : Site
  channel : ChannelName
  deriving DecidableEq, Ord, Repr, Inhabited

instance : ToString Slot := ⟨fun slot => s!"{slot.site}/{slot.channel}"⟩

/-- Why an endpoint is allowed on a channel. -/
inductive Standing where
  | root
  | granted (abilities : Abilities)
  deriving DecidableEq, Repr, Inhabited

/-- An invitation that has been minted, and what became of it. -/
structure Mint where
  minter : Slot
  grants : Abilities
  living : Bool
  spent : Bool
  deriving DecidableEq, Repr, Inhabited

/-- One side of one channel. -/
structure Chan where
  standing : Standing
  far : Option Slot
  met : Bool
  said : List String
  cut : Bool
  deriving DecidableEq, Repr, Inhabited

private def opened (standing : Standing) (far : Option Slot) (met : Bool) : Chan :=
  { standing, far, met, said := [], cut := false }

/-- Everything a person could know after a trace. -/
structure World where
  minted : TreeMap Nat Mint := ∅
  channels : TreeMap Slot Chan := ∅
  deriving Inhabited

/-- One thing to try, as the model names it. -/
inductive Action where
  | invite (site : Site) (channel : ChannelName) (lifetime : Lifetime) (abilities : Abilities)
  | join (site : Site) (ticket : Var) (channel : ChannelName)
  | send (site : Site) (channel : ChannelName) (text : String)
  | read (site : Site) (channel : ChannelName)
  | revoke (site : Site) (channel : ChannelName)
  deriving DecidableEq, Repr, Inhabited

instance : BEq Action := ⟨fun left right => decide (left = right)⟩

def Action.described : Action → String
  | .invite site channel lifetime abilities =>
    s!"Invite {site} {channel} {repr lifetime} send={abilities.maySend} read={abilities.mayRead}"
  | .join site ticket channel => s!"Join {site} {ticket} {channel}"
  | .send site channel text => s!"Send {site} {channel} {text}"
  | .read site channel => s!"Read {site} {channel}"
  | .revoke site channel => s!"Revoke {site} {channel}"

private def World.at? (world : World) (slot : Slot) : Option Chan := world.channels.get? slot

private def World.alter (world : World) (slot : Slot) (change : Chan → Chan) : World :=
  { world with channels := world.channels.alter slot (·.map change) }

/-- What a use of a channel needs from the standing that allows it. -/
inductive Use where
  | sending
  | reading

def permits : Use → Standing → Bool
  | _, .root => true
  | .sending, .granted abilities => abilities.maySend
  | .reading, .granted abilities => abilities.mayRead

/--
The refusal the model says a caller must see, if any.

The order of the guards is the order the program checks in. Getting that order
wrong would not weaken the property — it would make the adversary demand a
different failure than the one the caller is entitled to.
-/
def refusal (world : World) : Action → Option Code
  | .invite site channel _ _ =>
    if (world.at? ⟨site, channel⟩).isSome then some ⟨"kusanagi.channel_exists"⟩ else none
  | .join site ticket channel =>
    if (world.at? ⟨site, channel⟩).isSome then some ⟨"kusanagi.channel_exists"⟩
    else match world.minted.get? ticket.step with
      | none => none
      | some mint =>
        -- Found by this adversary, then fixed in Rust: an endpoint that
        -- accepted its own invitation held two local names for one stream and
        -- read its own segments back as a peer's.
        if mint.minter.site == site then some ⟨"kusanagi.own_invitation"⟩
        else if !mint.living then some ⟨"grant.expired"⟩
        else if mint.spent then some ⟨"kusanagi.invite_spent"⟩
        else none
  | .send site channel _ =>
    match world.at? ⟨site, channel⟩ with
    | none => some ⟨"kusanagi.unknown_channel"⟩
    | some chan =>
      if !permits .sending chan.standing then some ⟨"grant.forbidden"⟩
      else if chan.cut then some ⟨"grant.revoked"⟩
      else
        -- Found by this adversary, then fixed in Rust: a segment the peer may
        -- no longer read is not written. Revocation cuts both directions, and
        -- a segment is filed in its reader's ward, so with nobody to read it
        -- there is nowhere to write it (D-20).
        match chan.far.bind world.at? with
        | none => some ⟨"kusanagi.no_peer_yet"⟩
        | some far => if permits .reading far.standing then none else some ⟨"grant.forbidden"⟩
  | .read site channel =>
    match world.at? ⟨site, channel⟩ with
    | none => some ⟨"kusanagi.unknown_channel"⟩
    | some chan =>
      if !permits .reading chan.standing then some ⟨"grant.forbidden"⟩
      else if chan.cut then some ⟨"grant.revoked"⟩
      else
        match chan.far.bind world.at? with
        -- Nobody has accepted, so there is nobody to have written anything.
        | none => some ⟨"kusanagi.no_peer_yet"⟩
        -- Somebody accepted, but an endpoint that may not write cannot be met
        -- either: the greeting that would introduce them is refused by the
        -- same authority that would refuse their segments.
        | some far => if permits .sending far.standing then none else some ⟨"grant.forbidden"⟩
  | .revoke site channel =>
    match world.at? ⟨site, channel⟩ with
    | none => some ⟨"kusanagi.unknown_channel"⟩
    | some chan =>
      if !chan.met then some ⟨"kusanagi.no_peer_yet"⟩
      else if chan.standing == .root then none
      else some ⟨"kusanagi.cannot_revoke_root"⟩

/--
What a read must return: exactly what the far side said, in order.

This is the one liveness property, and it is a relation between two traces —
what went in on one endpoint and what came out on the other — rather than a
recomputation of anything.
-/
def expected (world : World) (slot : Slot) : List String :=
  (do
    let chan ← world.at? slot
    let far ← chan.far
    return (← world.at? far).said).getD []

private def aName : Gen ChannelName := Gen.elements ⟨"one"⟩ [⟨"two"⟩]
private def aSite : Gen Site := Gen.elements .alice [.bob, .mallory]
private def aText : Gen String := Gen.elements "alpha" ["beta", "gamma"]
private def aLifetime : Gen Lifetime := Gen.frequency (9, pure .forever) [(1, pure .instantly)]
private def anAbility : Gen Abilities :=
  Gen.elements both [both, sendOnly, readOnly, neither]

instance : StateModel World Action where
  initial := {}

  arbitraryAction world := do
    let tickets := world.minted.keys
    let common : List (Nat × Gen Action) :=
      [ (3, do return .invite (← aSite) (← aName) (← aLifetime) (← anAbility))
      , (5, do return .send (← aSite) (← aName) (← aText))
      , (6, do return .read (← aSite) (← aName))
      , (2, do return .revoke (← aSite) (← aName)) ]
    let joining : List (Nat × Gen Action) :=
      match tickets with
      | [] => []
      | first :: rest =>
        [(6, do return .join (← aSite) ⟨← Gen.elements first rest⟩ (← aName))]
    match common ++ joining with
    | [] => return none
    | head :: rest => return some (← Gen.frequency head rest)

  -- Only an invitation that was minted earlier in the trace can be presented; a
  -- shrunk trace that dropped the minting must drop the acceptance with it.
  attemptable world
    | .join _ ticket _ => world.minted.contains ticket.step
    | _ => true

  precondition world action := (refusal world action).isNone

  -- An action the model expects to be refused is worth running, because the
  -- refusal is the promise. Running it as a negative action is how the door's
  -- error codes get tested at all.
  validFailingAction world action := (refusal world action).isSome

  next world action ticket :=
    match action with
    | .invite site channel lifetime abilities =>
      { minted := world.minted.insert ticket.step
          { minter := ⟨site, channel⟩, grants := abilities
          , living := lifetime == .forever, spent := false }
        channels := world.channels.insert ⟨site, channel⟩ (opened .root none false) }
    | .join site held channel =>
      match world.minted.get? held.step with
      | none => world
      | some mint =>
        let linked := world.channels.alter mint.minter
          (·.map fun far => { far with far := some ⟨site, channel⟩ })
        { minted := world.minted.insert held.step { mint with spent := true }
          channels := linked.insert ⟨site, channel⟩
            (opened (.granted mint.grants) (some mint.minter) true) }
    -- A send meets the peer the way a read does: a drop is filed where its
    -- reader looks, so a writer that has not met its reader looks them up first.
    | .send site channel text =>
      world.alter ⟨site, channel⟩ fun chan => { chan with said := chan.said ++ [text], met := true }
    | .read site channel => world.alter ⟨site, channel⟩ ({ · with met := true })
    | .revoke site channel => world.alter ⟨site, channel⟩ ({ · with cut := true })

  described := Action.described

/-- What it takes to run a trace: a built binary and a world to run it in. -/
structure Kit where
  door : Door
  ground : Ground

/-- What a step handed back, in the only shapes this model's actions produce. -/
inductive Realized where
  | minted (invitation : Invitation)
  | done
  | texts (said : List String)
  deriving Repr, Inhabited

private def Kit.attempt (kit : Kit) (site : Site) (verb : Verb)
    (project : Outcome → Option Realized) : IO (Except Complaint Realized) := do
  match ← Door.ask kit.door (kit.ground.siteOf site) verb with
  | .refused complaint => return .error complaint
  | .accepted outcome =>
    match project outcome with
    | some value => return .ok value
    | none =>
      throw <| IO.userError
        s!"the door answered a question other than the one asked: {repr outcome}"

instance : RunModel Kit World Action Realized where
  perform kit _ action look :=
    match action with
    | .invite site channel lifetime abilities =>
      kit.attempt site (.invite channel kit.ground.waypoint lifetime abilities) fun
        | .invited _ invitation _ => some (.minted invitation)
        | _ => none
    | .join site ticket channel =>
      match look ticket with
      | some (.minted invitation) =>
        kit.attempt site (.join invitation channel) fun
          | .joined .. => some .done
          | _ => none
      | _ => pure (.ok .done)
    | .send site channel text =>
      kit.attempt site (.send channel text) fun
        | .sent .. => some .done
        | _ => none
    | .read site channel =>
      kit.attempt site (.read channel) fun outcome => (Answer.heard outcome).map .texts
    | .revoke site channel =>
      kit.attempt site (.revoke channel) fun
        | .revoked .. => some .done
        | _ => none

  postcondition before _ action realized :=
    match action, realized with
    | .read site channel, .texts said =>
      let owed := expected before ⟨site, channel⟩
      pure (ensure (said == owed) s!"heard {said} where {owed} was said")
    | _, _ => pure .held

  postconditionOnFailure before action outcome :=
    let owed := (refusal before action).map Code.stable
    match outcome with
    | .ok _ => pure (.broke s!"this was accepted, and {owed} was owed")
    | .error complaint =>
      pure (ensure (some complaint.code.stable == owed)
        s!"refused with {complaint.code} where {owed} was owed")

/--
Any prefix, one revocation, any suffix, and a read that must still fail.

Uniform random traces reach this state rarely and by accident. Naming the attack
and quantifying over what surrounds it is the difference between a fuzzer and an
adversary, and it is the reason this hunt is written against a model with
dynamic logic rather than against a generator.
-/
def revocationIsFinal : Script World Action Unit := do
  let one : ChannelName := ⟨"one"⟩
  let ticket ← Script.step (.invite .alice one .forever both)
  let _ ← Script.step (.join .bob ticket one)
  let _ ← Script.step (.read .alice one)
  Script.anyActions
  let _ ← Script.step (.revoke .alice one)
  Script.anyActions
  let world ← Script.modelState
  if ((world.channels.get? ⟨.alice, one⟩).map Chan.cut).getD false then
    Script.failing (.read .alice one)

end Kusanagi.Model
