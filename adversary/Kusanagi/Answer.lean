/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Lean.Data.Json

/-!
# The algebraic mirror of what the door says

This module parses and nothing else. It holds no opinion about whether an
answer is right, because an opinion here would be a second authority for a
rule that already lives in Rust.

An unknown `command` tag is a parse failure rather than a shrug: the door has
changed shape, and a test that quietly treated a new shape as a refusal would
report green for a product it can no longer see.
-/

namespace Kusanagi.Answer

open Lean (Json FromJson fromJson?)

/-- A handle is a public key, rendered. -/
structure Handle where
  rendered : String
  deriving DecidableEq, Ord, Repr, Inhabited

/-- What a channel is called on one endpoint. Names are local, not shared. -/
structure ChannelName where
  said : String
  deriving DecidableEq, Ord, Repr, Inhabited

/-- One line of text that admits exactly one endpoint to one channel. -/
structure Invitation where
  line : String
  deriving DecidableEq, Ord, Repr, Inhabited

/-- An opaque place on a host where exactly one segment lives. -/
structure Address where
  key : String
  deriving DecidableEq, Ord, Repr, Inhabited

/-- The stable identifier of a failure, such as `grant.revoked`. -/
structure Code where
  stable : String
  deriving DecidableEq, Ord, Repr, Inhabited

instance : FromJson Handle where fromJson? j := Handle.mk <$> fromJson? j
instance : FromJson ChannelName where fromJson? j := ChannelName.mk <$> fromJson? j
instance : FromJson Invitation where fromJson? j := Invitation.mk <$> fromJson? j
instance : FromJson Address where fromJson? j := Address.mk <$> fromJson? j
instance : FromJson Code where fromJson? j := Code.mk <$> fromJson? j

instance : ToString Handle := ⟨Handle.rendered⟩
instance : ToString ChannelName := ⟨ChannelName.said⟩
instance : ToString Invitation := ⟨Invitation.line⟩
instance : ToString Address := ⟨Address.key⟩
instance : ToString Code := ⟨Code.stable⟩

/--
The value at `key`, where absent and `null` mean the same thing.

The door omits a field it has nothing to say about, but a JSON writer is free
to send `null` instead, and the two must not be a difference this adversary can
see — that difference belongs to no rule anybody wrote.
-/
def optional (j : Json) (α : Type) [FromJson α] (key : String) : Except String (Option α) :=
  match j.getObjVal? key with
  | .error _ => .ok none
  | .ok .null => .ok none
  | .ok found => (fromJson? found : Except String α).map some

/--
A height, an index or a count, as the door writes it.

The door writes all of them as JSON numbers. Lean's own `UInt64` reader expects
a string, because it serialises through one so that a value above `2^53`
survives a reader that keeps numbers as doubles. The two are bridged here rather
than by an instance: an instance would change what `UInt64` means for every
other module that ever reads JSON in this tree.
-/
def counted (j : Json) (key : String) : Except String UInt64 :=
  (j.getObjValAs? Nat key).map UInt64.ofNat

/-- The same, where the door omits the field or writes `null` for "none". -/
def counted? (j : Json) (key : String) : Except String (Option UInt64) :=
  (optional j Nat key).map (·.map UInt64.ofNat)

/--
What a segment carried, in the one encoding that does not lose it.

Which of the two arrives is a fact about the bytes rather than a choice: a
payload that is valid UTF-8 survives a JSON string intact, and one that is not
cannot go in a string at all.
-/
inductive Carried where
  | asText (said : String)
  /-- The exact bytes, in lowercase hexadecimal. -/
  | asBytes (hex : String)
  deriving DecidableEq, Repr, Inhabited

/-- What a segment carried, however it was rendered. -/
def Carried.shown : Carried → String
  | .asText said => said
  | .asBytes hex => hex

/-- One segment as the door reports it: a height and what it carried. -/
structure Entry where
  index : UInt64
  carried : Carried
  deriving DecidableEq, Repr, Inhabited

instance : FromJson Entry where
  fromJson? j := do
    let index ← counted j "index"
    let text ← optional j String "text"
    let payload ← optional j String "payload"
    match text, payload with
    | some said, none => .ok { index, carried := .asText said }
    | none, some hex => .ok { index, carried := .asBytes hex }
    | _, _ =>
      .error "a segment carried both renderings or neither; the door promises exactly one"

/-- One channel, as it is listed. -/
structure Summary where
  name : ChannelName
  standing : String
  peer : Option String
  /-- The name the peer signed for itself, once it arrived; nothing else. -/
  alias? : Option String
  deriving DecidableEq, Repr, Inhabited

instance : FromJson Summary where
  fromJson? j := do
    let name ← j.getObjValAs? ChannelName "name"
    let standing ← j.getObjValAs? String "standing"
    let peer ← optional j String "peer"
    let alias? ← optional j String "alias"
    .ok { name, standing, peer, alias? }

/-- Where one member's copy of a fan-out went, or why it did not. -/
structure Landed where
  member : ChannelName
  status : String
  code : Option Code
  address : Option Address
  deriving DecidableEq, Repr, Inhabited

instance : FromJson Landed where
  fromJson? j := do
    let member ← j.getObjValAs? ChannelName "member"
    let status ← j.getObjValAs? String "status"
    let code ← optional j Code "code"
    let address ← optional j Address "address"
    .ok { member, status, code, address }

/-- What the program reports when it did what was asked. -/
inductive Outcome where
  | identity (handle : Handle)
  | channels (listed : List Summary)
  | invited (name : ChannelName) (invitation : Invitation) (expiresAt : UInt64)
  | joined (name : ChannelName) (handle peer : Handle)
  | sent (name : ChannelName) (index : UInt64) (address : Address)
  /--
  The channel, the handle that signed every segment reported, the verified
  head, and the segments themselves.
  -/
  | read (name : ChannelName) (author : Handle) (height : Option UInt64) (segments : List Entry)
  /-- The channel, and how many payloads are now waiting for a slot. -/
  | queued (name : ChannelName) (waiting : UInt64)
  /-- The channel, the slot, and the height written if one was. -/
  | ticked (name : ChannelName) (slot : UInt64) (wrote : Option UInt64)
  | revoked (name : ChannelName) (step : String)
  /-- The channel dropped here, and the locator its drops stay at. -/
  | forgotten (name : ChannelName) (waypoint : String)
  | examined (waypoint tier : String)
  | hosted
  /-- The group, and the channels it now stands for. -/
  | grouped (group : ChannelName) (members : List ChannelName)
  /-- The group, and where each member's copy landed. -/
  | fannedOut (group : ChannelName) (delivered : List Landed)
  /-- The recovery key, said once. The archive itself is on stdout. -/
  | exported (recovery : String)
  /-- Where the site was restored, and how many channels came with it. -/
  | imported (site : String) (channels : UInt64)
  deriving DecidableEq, Repr, Inhabited

instance : FromJson Outcome where
  fromJson? j := do
    match ← j.getObjValAs? String "command" with
    | "identity" => return .identity (← j.getObjValAs? Handle "handle")
    | "channels" => return .channels (← j.getObjValAs? (List Summary) "channels")
    | "invited" =>
      return .invited (← j.getObjValAs? ChannelName "name") (← j.getObjValAs? Invitation "invite")
        (← counted j "expires_at")
    | "joined" =>
      return .joined (← j.getObjValAs? ChannelName "name") (← j.getObjValAs? Handle "handle")
        (← j.getObjValAs? Handle "peer")
    | "sent" =>
      return .sent (← j.getObjValAs? ChannelName "name") (← counted j "index")
        (← j.getObjValAs? Address "address")
    | "read" =>
      return .read (← j.getObjValAs? ChannelName "name") (← j.getObjValAs? Handle "author")
        (← counted? j "height") (← j.getObjValAs? (List Entry) "segments")
    | "queued" =>
      return .queued (← j.getObjValAs? ChannelName "name") (← counted j "waiting")
    | "ticked" =>
      return .ticked (← j.getObjValAs? ChannelName "name") (← counted j "slot")
        (← counted? j "wrote")
    | "revoked" =>
      return .revoked (← j.getObjValAs? ChannelName "name") (← j.getObjValAs? String "step")
    | "forgotten" =>
      return .forgotten (← j.getObjValAs? ChannelName "name") (← j.getObjValAs? String "waypoint")
    | "examined" =>
      return .examined (← j.getObjValAs? String "waypoint") (← j.getObjValAs? String "tier")
    | "hosted" => return .hosted
    | "grouped" =>
      let group ← j.getObjVal? "group"
      return .grouped (← group.getObjValAs? ChannelName "name")
        (← group.getObjValAs? (List ChannelName) "members")
    | "fanned_out" =>
      return .fannedOut (← j.getObjValAs? ChannelName "group")
        (← j.getObjValAs? (List Landed) "delivered")
    | "exported" => return .exported (← j.getObjValAs? String "recovery")
    | "imported" =>
      return .imported (← j.getObjValAs? String "site") (← counted j "channels")
    | other => .error s!"the door reported a command this adversary does not know: {other}"

/--
What the program reports when it could not.

Every field is load-bearing: the code is what a machine acts on, and the
recovery is the reason a code is worth having.
-/
structure Complaint where
  code : Code
  message : String
  recover : String
  deriving DecidableEq, Repr, Inhabited

instance : FromJson Complaint where
  fromJson? j := do
    let code ← j.getObjValAs? Code "code"
    let message ← j.getObjValAs? String "error"
    let recover ← j.getObjValAs? String "recover"
    .ok { code, message, recover }

/-- The answer to one question, on whichever stream carried it. -/
inductive Answer where
  | accepted (outcome : Outcome)
  | refused (complaint : Complaint)
  deriving DecidableEq, Repr, Inhabited

/--
Parses one stream.

The door writes UTF-8 on both streams, so bytes that are not UTF-8 are already
a door this adversary cannot read, and saying which is more use than a decoder
that substitutes replacement characters and then fails at the JSON layer.
-/
def decode (α : Type) [FromJson α] (raw : ByteArray) : Except String α :=
  match String.fromUTF8? raw with
  | none => .error "the stream was not UTF-8"
  | some text => do Lean.fromJson? (← Json.parse text)

def decodeOutcome (raw : ByteArray) : Except String Outcome := decode Outcome raw

def decodeComplaint (raw : ByteArray) : Except String Complaint := decode Complaint raw

/-- The texts of a read, in order. Anything else is not a read. -/
def heard : Outcome → Option (List String)
  | .read _ _ _ segments => some (segments.map (·.carried.shown))
  | _ => none

end Kusanagi.Answer
