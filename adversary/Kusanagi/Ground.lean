/-
This Source Code Form is subject to the terms of the Mozilla Public
License, v. 2.0. If a copy of the MPL was not distributed with this
file, You can obtain one at https://mozilla.org/MPL/2.0/.
Copyright (c) 2026 2youg1 and the kusanagi contributors
-/
import Kusanagi.Answer

/-!
# A world that exists for one test, and the host's power to lie in it

The host is the untrusted half of this network, so reaching into its directory
is not going behind the product's back — it is taking the position the threat
model already grants an adversary. Everything an endpoint does still goes
through `Kusanagi.Door`.
-/

namespace Kusanagi.Ground

open Kusanagi.Answer

/--
The cast. Three endpoints are enough for every question a two-party channel can
raise: two to talk, and one to try what it was never invited to.
-/
inductive Site where
  | alice
  | bob
  | mallory
  deriving DecidableEq, Ord, Repr, Inhabited

def cast : List Site := [.alice, .bob, .mallory]

def Site.named : Site → String
  | .alice => "alice"
  | .bob => "bob"
  | .mallory => "mallory"

instance : ToString Site := ⟨Site.named⟩

/-- One throwaway world: a host nobody trusts, and the sites that use it. -/
structure Ground where
  root : System.FilePath
  host : System.FilePath
  deriving Repr, Inhabited

/-- Where one endpoint keeps its identity and channels. -/
def Ground.siteOf (ground : Ground) (site : Site) : System.FilePath :=
  ground.root.join site.named

/-- The host, as a locator an endpoint can be pointed at. -/
def Ground.waypoint (ground : Ground) : System.FilePath := ground.host

/-- Runs an action in a world that is deleted afterwards. -/
def withGround (act : Ground → IO α) : IO α := do
  let root ← IO.FS.createTempDir
  let ground : Ground := { root, host := root.join "host" }
  IO.FS.createDirAll ground.host
  try
    act ground
  finally
    -- A world that survived its test would be a fact the next one inherits.
    try IO.FS.removeDirAll root catch _ => pure ()

/--
Where a host keeps one object.

An address here is the whole key a host files a drop under,
`period/ward/address`, which is what a send reports and what a directory host
uses as a path. That is the one implementation detail this adversary depends
on; when it changes, these properties fail loudly rather than silently testing
nothing.
-/
def Ground.placed (ground : Ground) (address : Drop) : System.FilePath :=
  (address.key.splitOn "/").foldl (fun below part => below.join (System.FilePath.mk part)) ground.host

/-- The bin a key sits in: everything before the address. -/
def binOf (address : Drop) : String :=
  let parts := address.key.splitOn "/"
  String.intercalate "/" (parts.take (parts.length - 1))

/--
Everything the host holds, which is everything the host knows.

Sorted, so that a property about the host's view does not accidentally depend
on the order a directory happened to be walked in.
-/
partial def Ground.stored (ground : Ground) : IO (List (Drop × ByteArray)) := do
  let rec walk (directory : System.FilePath) : IO (List (Drop × ByteArray)) := do
    let mut found := []
    for entry in ← directory.readDir do
      if entry.fileName == ".staging" then
        continue
      if ← entry.path.isDir then
        let below ← walk entry.path
        found := found ++ below.map fun (address, bytes) =>
          (⟨entry.fileName ++ "/" ++ address.key⟩, bytes)
      else
        found := found ++ [(⟨entry.fileName⟩, ← IO.FS.readBinFile entry.path)]
    return found
  -- One address holds one object, so ordering by the key alone is a total order
  -- over what the host has.
  return (← walk ground.host).mergeSort fun left right => left.1.key ≤ right.1.key

/-- The bytes the host holds at one address. -/
def Ground.holding (ground : Ground) (address : Drop) : IO ByteArray :=
  IO.FS.readBinFile (ground.placed address)

/-- Flips one bit of an object, the way damage or a hostile host would. -/
def Ground.corrupt (ground : Ground) (address : Drop) : IO Unit := do
  let path := ground.placed address
  let bytes ← IO.FS.readBinFile path
  if h : 0 < bytes.size then
    IO.FS.writeBinFile path (bytes.set 0 (bytes[0]'h + 1))
  else
    throw <| IO.userError s!"the host is holding nothing at {address}"

/--
Changes the byte at one offset, which is how damage and a hostile host both
look from the reader's side. Offsets past the end change nothing.
-/
def Ground.damage (ground : Ground) (offset : Nat) (address : Drop) : IO Unit := do
  let path := ground.placed address
  let bytes ← IO.FS.readBinFile path
  if h : offset < bytes.size then
    IO.FS.writeBinFile path (bytes.set offset (bytes[offset]'h + 1))

/--
Puts whatever bytes the host likes at an address, whether or not anything was
there. A host that can write its own disk can do this; the question is only
ever what a reader makes of it.
-/
def Ground.plant (ground : Ground) (address : Drop) (bytes : ByteArray) : IO Unit := do
  let path := ground.placed address
  if let some parent := path.parent then
    IO.FS.createDirAll parent
  IO.FS.writeBinFile path bytes

/--
Drops an object the host was holding.

A host cannot forge a segment, but it can always refuse to hand one over, and
refusing selectively is a lie about history rather than an outage. Nothing
stops it; what it must not achieve is a reader believing less than that reader
has already verified.
-/
def Ground.vanish (ground : Ground) (address : Drop) : IO Unit :=
  IO.FS.removeFile (ground.placed address)

/--
Serves the object from one address at another.

The strongest move a store gets for free. It forges nothing and corrupts
nothing: every byte it hands over is a byte an endpoint really wrote and really
signed. What it changes is only *where* those bytes are, and this network
answers that with the key rather than with a check — an address derives the key
its contents are sealed under, so bytes that arrive at the wrong address do not
open at all.
-/
def Ground.transplant (ground : Ground) (from' to : Drop) : IO Unit := do
  IO.FS.writeBinFile (ground.placed to) (← IO.FS.readBinFile (ground.placed from'))

end Kusanagi.Ground
