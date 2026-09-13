# GLOSSARY — the words

One name per concept, and one concept per name. A word enters this table only
with the declaration that implements it, and `just glossary` refuses a tree where
the two have parted. The last column is what the word fixes: it is what a synonym
would lose, and what the next reader needs in order to re-argue the name.
`ARCHITECTURE.md` says why the network is shaped as it is; this file says what
its parts are called.

## 1 The words

| Word | In the code | What it is | What it fixes |
|---|---|---|---|
| **Segment** | `kernel::Segment` | the only thing that travels: hash-linked bytes, signed at genesis and trail-proven after | ordering, replay and fork detection are one check on one structure |
| **Message** | `walk::Message` | one thing an author meant to say: one segment, or a run of up to 32 on a channel and 64 in a room, joined in-band; a run that has half arrived is neither reported nor stored | long content needs no second key space, so reading it is not a pairing signal (D-21) |
| **Drop** | `kernel::DropAddr` | an opaque address that receives exactly one segment; the type carries a suffix because `Drop` is a `std` trait | addresses never repeat, so relationships never appear |
| **Stream** | `seal::Stream` | one author's sequence of drops inside a channel | two people sharing one secret never contend for an address |
| **Trail** | `kernel::Trail` | one author's private sequence of one-time proofs for one stream: each segment shows the current proof and commits to the next | a peer can check who wrote a message and can never prove it to anybody else |
| **Freight** | `kernel::Freight` | what a segment carries besides its place in the chain: the payload, the purpose, and how far its author had verified the other side | three facts one caller settles at once, so two of them cannot disagree |
| **Filler** | `kernel::Purpose::Filler` | a segment written because a slot came round and nothing was queued; sealed, chained and counted like any other, never reported | an endpoint with everything to say and one with nothing produce the same traffic |
| **Veil** | `seal::Fit::Veil` | the one size every sealed drop has: 131 072 bytes, which is `seal::DROP`, under a checked pad, no exceptions | how much was said stops being a thing anybody holds |
| **Ratchet** | `seal::Ratchet` | a key that only moves forward, its predecessor overwritten | a host that kept a copy of a released drop holds bytes nobody can open (D-01) |
| **Handle** | `kernel::Handle` | the BLAKE3 hash of an endpoint's public key, 32 bytes: how an endpoint is named everywhere | a name checks nothing, so the key travels only to where a signature is checked |
| **Alias** | `kernel::Alias` | what an endpoint asks to be called, signed by its key, carried sealed in every later invitation and greeting | a label is printed beside output and never stands in for a handle |
| **Waypoint** | `kernel::Waypoint` | anything that stores bytes under a key | the store is never trusted; everything is checked against a hash |
| **Locator** | `waypoint::Locator` | where a waypoint is, as the string an invitation carries: a directory, `http(s)://`, `s3://` or `carry://` | whoever holds the line can open the place, and nothing else is needed to reach it |
| **Carrier** | `waypoint::Carrier` | a real client of a real service, invoked rather than imitated, that moves the bytes | what crosses the network is that client's traffic because it is |
| **Box** | `box::Server` | a host somebody runs: it holds sealed bytes at opaque addresses and refuses to overwrite one | the untrusted half is a program, not a promise |
| **Ward** | `kernel::Ward` | sixteen bits a reader draws at random when its identity is made and hands to every writer: which corner of a host its drops are filed in | a read names a crowd and never an address; the crowd is worth the number of readers who share it (D-20) |
| **Period** | `kernel::Period` | which stretch of public time a drop was written in; part of its key on the host | a bin is finite, and one lifetime covers every object in it |
| **Bin** | `kernel::Bin` | one period of one ward: everything a reader takes in one request | what a reader asks for is a function of public data only |
| **Sweep** | `kernel::Sweep` | the set of bins one read asks for, named by the leading hex digits of a ward; fewer digits is a larger crowd at more bandwidth | the reader turns that knob alone; no writer, host or peer is consulted |
| **Cairn** | `chain::Cairn` | how far one author's stream has been verified: a handle and a head, 73 bytes | a reader resumes instead of re-naming a stream, and cannot be talked back down below it |
| **Grant** | `grant::Grant` | offline-verifiable authority that can only narrow | permission exists in this form and no other |
| **Standing** | `site::Standing` | why somebody is allowed on a channel — root, or granted | "the authority holds no grant" is a fact, not a missing value |
| **Channel** | `site::Channel` | one conversation: a secret, a locator, a standing, a peer | the unit an endpoint joins, lists, and revokes |
| **Peer** | `site::Peer` | the other end of a channel, once it has said who it is: its key, and therefore its handle | a channel has exactly one other side, and its record holds the key once |
| **Invite** | `site::Invite` | the one line that admits one peer: the channel secret, a locator and a suite byte | the secret is carried by the line; the public bytes are fetched from the offer |
| **Offer** | `site::Offer` | what an invitation points at rather than carries: the inviter's key, grant, alias, retention and ward, sealed in one drop the channel secret addresses | the secret stops being held hostage by the public bytes beside it |
| **Site** | `site::Site` | what one endpoint keeps on its own disk: a seed, a file per channel, a cairn per stream, a revocation list | the only state there is; anything else would be state a kill could lose |
| **Roster** | `site::Roster` | one endpoint's own list of the channels a group name stands for, replaced whole and shared with nobody; the verb is `group` | a small group needs no group key, no agreement and no removal protocol |
| **Room** | `site::Room` | one secret shared by up to 32 members, each writing its own stream in one ward; only the founder invites, and signs who is in | more than two parties without a group key, at a price paid knowingly: every member learns every other member's handle |
| **Cadence** | `site::Cadence` | how often an endpoint writes on a channel: on demand, or one drop every period | the rhythm of speech stops being a function of what there is to say (D-06) |
| **Retention** | `site::Retention` | what becomes of a drop once the peer acknowledges it: kept, or released | the combination that must not exist — release without a backup — is visible rather than accidental (D-07) |

A word here is declared as a type by exactly one crate. Every other type is a
crate's own, named in that crate's SPEC §6 命名统一; `door` names what a verb
reports in `door-SPEC.md`, and a report row is a rendering of a word above, not
a second word for it. The Lean and Zig sides coin no name for anything in this
table.

## 2 Reserved

Named in `ARCHITECTURE.md` §9 for work not yet done, and therefore declared
nowhere — not in Rust, not in Lean, not in Zig.

| Reserved | For |
|---|---|
| `Bell` | a host that can be asked to wait, so a poll becomes a wait |
| `Cohort` | a thousand endpoints joined by transitive grants |
| `Depot` | chunked content, closed by D-21 |

## 3 Not called

No name in the left column is declared as a type, enum, trait or alias anywhere
under `crates/*/src`. Each is what a newcomer reaches for; the right column is the
word already there.

| Not this | This |
|---|---|
| `Packet`, `Event`, `Post`, `Msg` | Segment, or Message |
| `Mailbox`, `Locker`, `DeadDrop` | Drop |
| `Feed`, `Timeline`, `Log` | Stream |
| `HashChain`, `Lamport`, `Proof` | Trail |
| `Cargo`, `Body`, `Content` | Freight |
| `Dummy`, `Cover`, `Chaff`, `Heartbeat`, `Noise` | Filler |
| `Padding`, `Pad` | Veil |
| `Rotation`, `Epoch` | Ratchet, or Period |
| `UserId`, `Fingerprint`, `Pubkey`, `PublicKey` | Handle, or `VerifyingKey` when the key itself is meant |
| `Nickname`, `Nick`, `DisplayName`, `Username` | Alias |
| `Store`, `Storage`, `Backend`, `Repository`, `Remote`, `Node` | Waypoint, or Box |
| `Transport`, `Courier`, `Mule` | Carrier |
| `Bucket`, `Shard`, `Partition`, `Prefix`, `Folder` | Ward, Period, or Bin |
| `Checkpoint`, `Bookmark`, `Cursor`, `Watermark` | Cairn |
| `Token`, `Permission`, `Macaroon`, `Caveat` | Grant |
| `Role`, `Rank` | Standing |
| `Conversation`, `Chat`, `Contact`, `Dialog` | Channel |
| `Friend`, `Counterparty`, `Partner`, `Buddy` | Peer |
| `Invitation`, `Ticket` | Invite |
| `Announcement`, `Advertisement` | Offer |
| `Home`, `Profile`, `Wallet`, `Keystore`, `Keychain`, `Account` | Site |
| `Group`, `Team`, `Squad`, `Members` | Roster |
| `Chamber`, `Lobby`, `Space` | Room |
| `Schedule`, `Interval`, `Timer` | Cadence |
| `Policy`, `Ttl` | Retention |

## 4 Two meanings today

Two words from §1 are declared a second time with a second meaning. Each row is
an exemption the gate reads, and each is closed the same way: a ruling on the new
name, the rename carried through every crate and SPEC in one change-set, and the
row deleted here.

| Word | Second declaration |
|---|---|
| `Roster` | `kernel::Roster`: who is in a room, as the founder signed it; travels as `Purpose::Roster` |
| `Standing` | `walk::Standing`: where a stream stands while a run of segments is built on it |

## 5 What the gate reads

`scripts/glossary.sh` is the one authority for holding this file against the
tree; `just glossary` and the check lane both call it. It keys on the header row
of each table above, so a table keeps its columns and may move.

- §1: the second column is one path. `crate::Name` must be declared under
  `crates/<crate>/src` as a struct, enum, trait, type alias or `identifier!`
  entry; `crate::Enum::Variant` must be a variant of an enum declared there. The
  word itself is declared as a type in at most one file of `crates/*/src`, except
  where §4 says otherwise.
- §2: no declaration under `crates/*/src`, `adversary/`, or `glass/src`.
- §3: every name in the left column is undeclared under `crates/*/src`.
- §4: the word in the left column may be declared twice; a third declaration
  is still refused.
