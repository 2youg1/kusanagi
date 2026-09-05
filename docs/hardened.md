# Stronger concealment, and a message larger than one drop

The README states what a host learns on the default path. This page is the next
rungs, and the size of one message. Nothing here changes the protocol.

## One ward is a shared bus

A period is ten minutes. A drop is 131 072 bytes. A bin that holds the default
cap of 256 drops, emptied every period, is about **54 KiB/s** — 256 × 131 072 /
600. Every reader of that ward downloads what every writer put in it. That
number is the bus. `kusanagi sweep --cap N` raises the ceiling a reader will
still take; it does not give anyone a private lane.

## How large one message may be

| Venue | Parts | Bytes |
|---|---:|---:|
| a channel | 32 | 4 042 720 |
| a room | 64 | 8 085 440 |

A segment still carries at most 126 339 bytes of payload. A larger message
becomes a run of ordinary segments on the author's own lane. The reader joins
them when the run completes. A run that never completes is not a message: it is
not reported, does not block the stream, and is not kept on disk.

Three reasons this is the shape, and not a second key space:

1. **A run of parts is the only reading of "one message" that does not invent a
   key space you fetch only because a secret named it.** That fetch is the
   pairing signal a sweep closed. Blobs, magnet-style addresses, and
   multi-round transfers stay out, whatever would carry them.
2. **The allowance follows who pays to download the bin.** A channel drop lands
   in the recipient's ward, shared with strangers who did not agree to this
   file, so 32 parts — one eighth of a default bin. A room drop lands in the
   room's own ward, paid for by people who joined, so 64 — one quarter.
3. **Past the limit, `send` is refused on this machine**, before any host is
   opened, with the exact byte count. The way out is volumes, each its own
   message.

A slotted channel (`--every`) still writes one drop per period. A multi-part
message on one is `kusanagi.slotted_one_drop`. A bin over the cap is
`kusanagi.ward_overfull` for that period, not a leak. `--cap` is 32–4096; 256
if unset.

## Larger than that, out of band

Three commands. The password is not the fourth.

```bash
7z a -p FILE.7z FILE
```

Stop `kusanagi host` if you were running one. The archive does not wait on that
box: a host you run is your address, and a download from it pairs the recipient
with every other request the box already saw.

```bash
kusanagi send --to NAME 'https://example/FILE.7z'
```

Say the password the way you said the four check digits — in person, not on
this channel. `split -b 4042720 FILE part.` makes volumes without a password;
they are then ordinary messages, each within the table above.

**Who is trusted, named.**

- The password: you and the recipient. It is worth nothing if it travels beside
  the archive.
- The place that holds the archive: that operator sees who uploaded, who
  downloaded, and how large it is.
- kusanagi's host: one more drop in a ward, not the file.
- Your own box, if you leave the file there: a new trust root that can pair the
  download with the rest of that box's traffic. That is why the second step is
  to stop it.

## A taller ladder

Without these rungs: content and size are hidden from the host; who reads whom
is a ward, not a pair; the host learns the IP unless a proxy is set.

| Rung | What you run | What it additionally hides | What it newly trusts |
|---|---|---|---|
| 1 Tor, required | `KUSANAGI_PROXY=socks5://127.0.0.1:9050` and `kusanagi proxy --require` | the cloud vendor and the ISP see Tor exits, not homes | nothing new: this is the network IP was already handed to |
| 2 Your box as an onion service | `kusanagi host` behind `HiddenServicePort 8963 127.0.0.1:8963`; invitations name `http://<name>.onion:8963` | the cloud vendor is out of the picture; the host is a machine you run | that machine, and Tor |
| 3 Your box in front of a bucket | **not built.** The box is already a host; it does not forward objects into a bucket | — | — |

Rung 3 as sometimes described — members reach your box over Tor, the box writes
a cloud bucket with its own key — would shrink the credential edge to
"everything this box wrote". The cost, said here so it cannot be the default:
the box sees ward-granularity times the way a cloud vendor does today, and it
is a single point of failure. Until that forward path exists, rung 3 is rung 2:
the box is the host, and there is no bucket.

Not on this ladder: mixnets, PIR, splitting one drop across hosts. Those
multiply observers or the bill.
