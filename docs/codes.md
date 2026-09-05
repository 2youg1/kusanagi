# Error codes

Every failure this program reports carries a **stable code**. A code never
changes its meaning once it is published, and it is the thing a script matches
on — the sentence beside it is for a person and may be reworded at any time.

**This table is checked by a test.** `crates/kusanagi/tests/codes.rs` walks every
`crates/*/src/**/*.rs`, collects every code literal, and requires the two sets to
be equal. Add a code without a row here, or delete a row without deleting the
code, and the build goes red with the difference printed. The code is the
authority; this file is the mirror a machine keeps honest.

Every `--json` object also carries `"contract": 1`, on success and on failure
alike. Adding a field does not move that number; removing or renaming one does.

The prefix says which layer noticed, not which layer a caller should act on:
`waypoint.*` is the host, `segment.*` and `chain.*` are bytes that arrived,
`grant.*` is authority, `seal.*` is decryption, `site.*` and `cairn.*` are this
disk, `locator.*` is what was typed, and `kusanagi.*` is the door itself.

| code | when | recover |
|---|---|---|
| `cairn.version` | a cairn file this build does not read | delete it; the next read walks from genesis and writes a new one |
| `cairn.width` | a cairn file is not the length a cairn is | delete it; the next read walks from genesis and writes a new one |
| `chain.author_changed` | two segments on one stream are signed by different handles | keep the bytes and report it: a host cannot do this, so somebody made two chains |
| `chain.exhausted` | the stream has reached the highest index this design can express | open a new channel; nothing can be appended to this one |
| `chain.expected_genesis` | the first segment of a stream is not a genesis | keep the bytes and report it |
| `chain.index_gap` | a segment does not sit one above the one before it | keep the bytes and report it: a host withheld a segment or replaced one |
| `chain.previous_mismatch` | a segment names a predecessor that is not the one before it | keep the bytes and report it |
| `chain.proof_refused` | the proof a segment reveals does not match what the one below it committed to | keep the bytes and report it |
| `chain.unexpected_genesis` | a genesis appears above height zero | keep the bytes and report it |
| `grant.detached` | a step was issued to somebody who does not hold the next one | ask whoever invited you for a new invitation |
| `grant.empty` | a grant chain has no steps in it | ask whoever invited you for a new invitation |
| `grant.expired` | the grant that authorises this has lapsed | ask whoever invited you for a new invitation |
| `grant.forbidden` | the grant does not carry the ability this verb needs | ask for an invitation that grants it |
| `grant.issuer_mismatch` | a step names an issuer who is not the holder of the step above it | ask whoever invited you for a new invitation |
| `grant.not_authentic` | a step's signature does not verify | keep the bytes and report it |
| `grant.not_the_holder` | this endpoint is not who the grant was issued to | use the endpoint the invitation was meant for |
| `grant.not_yours` | the grant chain does not descend from this channel's root | ask whoever invited you for a new invitation |
| `grant.revoked` | a step in the chain has been revoked here | nothing to do: the peer was cut off deliberately |
| `grant.too_long` | a grant chain declares more steps than this design allows | ask whoever invited you for a new invitation |
| `grant.trailing` | bytes follow a complete grant chain | keep the bytes and report it |
| `grant.truncated` | a grant chain ends in the middle of a step | keep the bytes and report it |
| `grant.unknown_ability` | a grant names an ability this build does not know | upgrade, or ask for an invitation this build can read |
| `grant.unknown_parent_tag` | a step's parent is neither a root nor a step | keep the bytes and report it |
| `grant.widened` | a step claims more than the step above it held | keep the bytes and report it: attenuation is one-way |
| `grant.wrong_root` | a grant chain is rooted in a handle that is not this channel's | ask whoever invited you for a new invitation |
| `kusanagi.address_unavailable` | this machine would not hand over the address `kusanagi host` was told to listen on | `--bind 0` takes any free port and prints it; `--bind ADDRESS` names one this machine has |
| `kusanagi.argument` | an argument is not something this verb can act on | the answer names the flag and what to pass instead |
| `kusanagi.needs_cairn` | a channel opened with `--release` was read, and the record of what had already been read is gone; the host no longer holds those drops | run `kusanagi import` with the archive `kusanagi export` made |
| `kusanagi.not_slotted` | `tick` was run on a channel that writes when it is asked to | send on it with `kusanagi send --to NAME`, or open the channel with `--every SECONDS` |
| `kusanagi.bad_recovery_key` | an archive did not open under the recovery key that was offered | check the key: it is the 64 hexadecimal digits `kusanagi export` printed once, and it goes in on the first line of stdin |
| `kusanagi.bad_greeting` | the introduction on a channel is not one this build can read | keep the bytes and report it |
| `kusanagi.cannot_revoke_root` | the peer of this channel is the authority that invited you | `kusanagi forget --channel NAME` instead |
| `kusanagi.channel_exists` | a channel of that name is already here | pick another name, or read the one you have |
| `kusanagi.drop_taken` | somebody claimed the next address first | read the channel to pick up the new head, then send again |
| `kusanagi.history_changed` | the host serves a history that contradicts one already verified here | run `kusanagi doctor` against the host; it just broke write-once |
| `kusanagi.invite_spent` | the invitation has already been accepted | ask for a fresh one; each admits exactly one endpoint |
| `kusanagi.local` | the operating system refused a read or a write | check that `--root` names a writable directory |
| `kusanagi.malformed` | a name, an invitation, or a file on this disk is not what it claims | the answer says which of the three, and what to do about it |
| `kusanagi.no_identity` | a channel was to be written before this endpoint had an identity | run `kusanagi id`, then try again |
| `kusanagi.no_invitation` | the invitation points at a drop the host does not have | ask for a fresh invitation: this one has expired, or the host no longer holds what it points at |
| `kusanagi.no_peer_yet` | nobody has joined this channel yet | wait, or run `kusanagi channels` to see what is here |
| `kusanagi.no_root` | the environment does not say where this user's data lives | pass `--root` |
| `kusanagi.not_the_peer` | a segment on the peer's stream was signed by somebody else | keep the bytes and report it |
| `kusanagi.own_invitation` | this invitation was minted by this endpoint | hand it to the endpoint you mean to admit |
| `kusanagi.unknown_channel` | no channel by that name is here | run `kusanagi channels` to see what is here |
| `kusanagi.unknown_group` | no group by that name has been made here | run `kusanagi channels` to see the groups, or write the roster with `kusanagi group --name NAME` |
| `locator.bad_proxy` | `KUSANAGI_PROXY` does not name a proxy | `socks5://host:port` or `http://host:port` |
| `locator.bucket_incomplete` | a bucket locator does not name a bucket | `s3://ENDPOINT/BUCKET[?region=R]` |
| `locator.carrier_missing` | a `carry://` locator was used and this machine has no carrier | set `KUSANAGI_CARRIER` to the program that moves the bytes |
| `locator.bad_carrier` | `KUSANAGI_CARRIER` is set to something that is not a program | it is a program name followed by any leading arguments, separated by spaces |
| `locator.credentials_missing` | a bucket was named without credentials | set `KUSANAGI_S3_ACCESS_KEY` and `KUSANAGI_S3_SECRET_KEY` |
| `locator.empty` | a waypoint was named as an empty string | a waypoint is a path, an `http://` url, or `s3://…` |
| `locator.unknown_scheme` | a locator names a scheme this build does not speak | a waypoint is a path, an `http://` url, or `s3://…` |
| `locator.network_path` | a directory locator is a network path (`\\host\share`, `//host/share`); opening it would be a connection the inviter chose, outside any proxy | mount the share yourself and name the drive or mount point it appears as |
| `seal.burned` | a key this endpoint destroyed on purpose was asked for again; the channel releases what its peer has read | restore from the archive `kusanagi export` made, or accept that those segments are gone |
| `seal.oversize` | a payload is larger than a drop can carry | send less in one segment |
| `seal.rejected` | sealed bytes did not open under the key this address derives | keep the bytes and report it |
| `seal.unusable` | the sealing key or nonce is not the width the cipher takes | report it: this is a defect, not an input |
| `segment.exhausted` | the stream has reached the highest index this design can express | open a new channel |
| `segment.follows_index` | a following segment claims height zero | keep the bytes and report it |
| `segment.genesis_index` | a genesis segment claims a height other than zero | keep the bytes and report it |
| `segment.not_authentic` | a genesis signature does not verify | keep the bytes and report it |
| `segment.not_the_author` | a segment names an author who is not the one expected here | keep the bytes and report it: the host served a stream nobody asked for |
| `segment.payload_too_large` | a segment declares a payload larger than one may carry | keep the bytes and report it |
| `segment.payload_unrepresentable` | a segment declares a payload larger than this machine can address | keep the bytes and report it |
| `segment.purpose` | a segment says it is neither a message nor a filler | keep the bytes and report it |
| `segment.tag` | a segment's first byte is neither genesis nor follows | keep the bytes and report it |
| `segment.trailing` | bytes follow a complete segment | keep the bytes and report it |
| `segment.truncated` | a segment ends in the middle of a field | keep the bytes and report it |
| `site.foreign_record` | a record on this disk was sealed by a platform store this one has not | run `kusanagi export` on the platform that made it, and pipe the archive into `kusanagi import` here |
| `site.permissions` | the operating system would not attach the restriction a site needs | choose a `--root` on a disk that keeps per-file permissions |
| `waypoint.io` | the underlying store failed | run `kusanagi doctor <waypoint>` |
| `waypoint.overwrite_not_refused` | the store accepted a write that should have been refused | this place cannot hold a channel; use one `doctor` passes |
| `waypoint.redirected` | the host answered with somewhere else to go, and was not followed | this host is not a box; check the waypoint url |
| `waypoint.timeout` | the host did not answer inside the deadline | retry; if it persists the host is down |
| `waypoint.unusable_address` | the address is not a usable key in this store | run `kusanagi doctor <waypoint>` |
| `waypoint.deletion_refused` | this kind of host cannot remove anything, so a channel that releases cannot keep its promise on it | open the channel without `--release`, or move it to a host that deletes |
| `waypoint.unwritten` | a write did not land, and the host said nothing about why | the host is full, or it is not a box; run `kusanagi doctor <waypoint>` |
