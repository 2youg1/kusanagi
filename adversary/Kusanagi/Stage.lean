/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer
import Kusanagi.Door
import Kusanagi.Ground

/-!
# Two endpoints put in conversation, and what each party to the threat model is then holding

Every property in the surface matrix begins the same way — somebody invites,
somebody joins, something is said — and then takes the position of one
adversary and looks at what that adversary has: the host's objects, a site's
bytes, a site's file names, an archive. This module is that beginning and
those positions, so that a property is only the relation it asserts.

Nothing here judges. A failure thrown out of `talk` means the stage could not
be set, which is a broken world rather than a finding.
-/

namespace Kusanagi.Stage

open Kusanagi.Answer
open Kusanagi.Door
open Kusanagi.Ground

/-- One channel between two sites, and what both ends said about it. -/
structure Talk where
  writer : System.FilePath
  reader : System.FilePath
  channel : ChannelName
  invitation : Invitation
  /-- The writer's handle, as the reader's `joined` reported it. -/
  writerHandle : Handle
  /-- The reader's handle, as its own `joined` reported it. -/
  readerHandle : Handle
  deriving Repr, Inhabited

/--
Opens a channel with whatever invitation the caller minted.

The verb must be an invitation on the channel; the reader joins under the same
local name, which keeps every property free of a second name to track.
-/
def talkWith (door : Door) (ground : Ground) (writer reader : Site) (minting : Verb) : IO Talk := do
  let minted ← Door.ask door (ground.siteOf writer) minting
  let (channel, invitation) ←
    match minted with
    | .accepted (.invited name line _) => pure (name, line)
    | other => throw <| IO.userError s!"the invitation was refused: {repr other}"
  let joined ← Door.ask door (ground.siteOf reader) (.join invitation channel)
  match joined with
  | .accepted (.joined _ own peer) =>
    return {
      writer := ground.siteOf writer
      reader := ground.siteOf reader
      channel
      invitation
      writerHandle := peer
      readerHandle := own }
  | other => throw <| IO.userError s!"the channel could not be joined: {repr other}"

/-- Opens an on-demand channel from one site to another on the ground's host. -/
def talk (door : Door) (ground : Ground) (writer reader : Site)
    (channel : ChannelName) : IO Talk :=
  talkWith door ground writer reader (.invite channel ground.waypoint .forever both)

/-- Says one thing from one site, and reports where the host was told to put it. -/
def say (door : Door) (site : System.FilePath) (channel : ChannelName)
    (text : String) : IO Address := do
  match ← Door.ask door site (.send channel text) with
  | .accepted (.sent _ _ address) => return address
  | other => throw <| IO.userError s!"a segment was refused: {repr other}"

/-- Reads the peer's stream from one site, and hands back the whole answer. -/
def hear (door : Door) (site : System.FilePath) (channel : ChannelName) : IO Answer :=
  Door.ask door site (.read channel)

/-- Reads this site's own stream, as the peer would. -/
def hearMine (door : Door) (site : System.FilePath) (channel : ChannelName) : IO Answer :=
  Door.ask door site (.readMine channel)

/-- The entries of a read, or why there were none. -/
def entriesOf : Answer → Except String (List Entry)
  | .accepted (.read _ _ _ entries) => .ok entries
  | other => .error s!"a read answered with something else: {repr other}"

/--
Every file under a site, with its bytes.

What a second account, a thief with the disk, or a subpoena is holding: all
of it, whatever the layout, so that a property does not have to know where
anything is kept.
-/
partial def siteBytes (root : System.FilePath) : IO (List (System.FilePath × ByteArray)) := do
  let rec walk (directory : System.FilePath) : IO (List (System.FilePath × ByteArray)) := do
    let mut found := []
    for entry in ← directory.readDir do
      if ← entry.path.isDir then
        found := found ++ (← walk entry.path)
      else
        found := found ++ [(entry.path, ← IO.FS.readBinFile entry.path)]
    return found
  if ← root.isDir then walk root else return []

/-- Every path component under a site, which is what a listing gives away. -/
partial def siteNames (root : System.FilePath) : IO (List String) := do
  let rec walk (directory : System.FilePath) : IO (List String) := do
    let mut found := []
    for entry in ← directory.readDir do
      let below ← if ← entry.path.isDir then walk entry.path else pure []
      found := found ++ (entry.fileName :: below)
    return found
  if ← root.isDir then walk root else return []

private def matchesAt (needle haystack : ByteArray) (offset : Nat) : Bool := Id.run do
  for index in [0:needle.size] do
    if needle[index]? != haystack[offset + index]? then
      return false
  return true

/-- Whether a needle occurs anywhere in a haystack of bytes. -/
def contains (needle haystack : ByteArray) : Bool := Id.run do
  if needle.size == 0 || needle.size > haystack.size then
    return false
  for offset in [0:haystack.size - needle.size + 1] do
    if matchesAt needle haystack offset then
      return true
  return false

/-- Which of the named haystacks hold the needle. -/
def anyContains (needle : ByteArray) (held : List (System.FilePath × ByteArray)) :
    List System.FilePath :=
  (held.filter fun (_, bytes) => contains needle bytes).map (·.1)

private def nibble (digit : UInt8) : UInt8 :=
  if digit ≥ 48 && digit ≤ 57 then digit - 48
  else if digit ≥ 97 && digit ≤ 102 then digit - 87
  else 0

private partial def unhex (digits : List UInt8) : List UInt8 :=
  match digits with
  | high :: low :: rest => (nibble high * 16 + nibble low) :: unhex rest
  | _ => []

/--
Lowercase hexadecimal as bytes, and its raw decoding, both of which a
leak could take the shape of.
-/
def hexOf (rendered : String) : List ByteArray :=
  let encoded := rendered.toUTF8
  [encoded, ByteArray.mk (unhex encoded.toList).toArray]

/-- Both shapes a handle can leak in. -/
def handleOf (handle : Handle) : List ByteArray := hexOf handle.rendered

/--
Both shapes the secret half of an invitation can leak in.

The payload after the scheme is a suite byte, a version byte, the 64 secret
bytes, and then the locator in the clear. Asserting that the whole payload
never appears is weaker than it looks — the locator is public — so this
hands back the secret's own 128 digits and a 32-digit slice from inside them,
which is what a partial leak would still contain.
-/
def secretOf (invitation : Invitation) : List ByteArray :=
  let payload := (invitation.line.dropWhile (· != ':')).drop 1
  let secret := (payload.drop 4).take 128 |>.toString
  [secret, (secret.drop 40).take 32 |>.toString].flatMap hexOf

/-- A channel name nobody else on the stage uses. -/
def fresh (said : String) : ChannelName := ⟨said⟩

end Kusanagi.Stage
