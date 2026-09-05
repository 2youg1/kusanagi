# Quickstart: one message, verified, in ten commands

This page gets two people (or two agents) from nothing to one delivered message.
One action per line. Each step ends with the line you should see; if you see it,
move on. If you do not, the fix is right there.

You need two computers, or two terminals on one computer. Call them **Alice**
and **Bob**. Every command below says which one it runs on.

Words you will meet, once each:

- **host** — the place messages wait until they are picked up. A folder both
  machines can reach, or a small program (`kusanagi host`) anybody can run. The
  host is never trusted: it stores locked boxes and cannot open them.
- **channel** — one conversation between you and one other person. You give it
  a name that only you see, like a contact in your phone.
- **invitation** — one long line of text. Whoever has it can join the channel,
  so hand it over the way you would hand over a key.
- **check code** — four characters both sides see. If they match, nobody
  altered the invitation on the way.

## 1. Build it (Alice and Bob)

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release
```

You should see, at the end: `Finished `release` profile [optimized] target(s)`.
The program is `target/release/kusanagi` (`kusanagi.exe` on Windows). Put it on
your PATH, or type the full path where the commands below say `kusanagi`.

Did not build? You need Rust 1.97 or newer: `rustup update`, then try again.

## 2. Say hello to yourself (Alice and Bob)

```bash
kusanagi id
```

You should see: `this endpoint is fdabe211…` — a long hexadecimal string. That
is your public name. It was just created; nothing was sent anywhere.

## 3. Pick a host (once, either side)

Any of these works. The simplest is the first.

**A folder both machines can see** — a synced folder (OneDrive, Dropbox,
iCloud, a network share mounted as a drive letter) or any directory when both
sides are the same computer. Write down its path; that path is your host.

**A program** — on any machine both sides can reach, run:

```bash
kusanagi host --bind 0.0.0.0:8963
```

You should see: `kusanagi host: serving … on 0.0.0.0:8963`. Your host is then
`http://THAT-MACHINE:8963`. Leave it running.

Not sure the host is good enough? `kusanagi doctor http://THAT-MACHINE:8963`
tells you, and says what is wrong if something is.

## 4. Alice invites Bob (Alice)

```bash
kusanagi invite --name bob --waypoint http://THAT-MACHINE:8963
```

(`--waypoint` takes the host from step 3: a URL, or a folder path.)

You should see: `channel `bob` is open`, then a line starting `kusanagi2:`,
then `check code 8e5c` (your four characters will differ).

Send the `kusanagi2:` line to Bob however you like — a message, a file, a QR
code, read aloud. Send the check code separately, or say it to Bob's face.

## 5. Bob joins (Bob)

Put the line in a file called `invite.txt`, then:

```bash
kusanagi join --name alice < invite.txt
```

You should see: `joined `alice``, then `check code 8e5c`. **Read the four
characters to Alice, and read her the `you` handle line.** Same on both sides
means the invitation arrived intact and the person who joined is you: the check
code proves the line was not altered, the handle proves who used it.
Different means somebody changed it: Alice runs step 4 again and you both
retry.

`invite_spent`? The line was already used — by you earlier, or by somebody who
intercepted it. Alice runs step 4 again.

## 6. Bob sends (Bob)

```bash
kusanagi send --to alice "hello from bob"
```

You should see: `sent on `alice` #0`. Number 0 is the first message on this
channel; the next will be 1.

## 7. Alice reads (Alice)

```bash
kusanagi read --from bob
```

You should see:

```
`bob`: fb32788bce33 verifies to height 0 (1 segment(s))
  #0   text, 14 bytes
<peer-8e3551dfd5c3cc06>
hello from bob
</peer-8e3551dfd5c3cc06>
```

`verifies` is the word that matters: the message was checked all the way back
to the first one, and it was Bob's. The `<peer-…>` lines are a fence: what is
between them came from Bob, everything outside came from the program.

`has written nothing yet`? Bob's message has not reached the host, or the host
is not the same on both sides. Both of you run `kusanagi channels` and compare
the last column: it must name the same host.

## 8. Alice answers (Alice)

```bash
kusanagi send --to bob "got it"
```

Bob reads with `kusanagi read --from alice`. That is a conversation.

Nothing can be sent before somebody joins: a message is filed where its reader
looks, and there is nowhere to file one for a reader who never arrived. `send`
on a channel nobody has joined is refused the way `read` is.

## 9. Keep a backup (Alice and Bob)

Your identity and channel keys live only on this machine. A lost disk is a lost
conversation, so:

```bash
kusanagi export > kusanagi-backup.ksnb
```

You should see, on the screen: `25396 bytes of archive are on stdout. The key
that opens them is` followed by a long hexadecimal key. **Write the key down
somewhere that is not this disk.** The file is useless without it, and the key
is useless without the file.

To move to a new machine: copy the file there, put the key on the first line
and the file after it, on standard input — never on the command line, which
other programs can read:

```bash
(echo YOUR-KEY; cat kusanagi-backup.ksnb) | kusanagi import
```

You should see: `restored 2 channel(s) into …`.

## That is all

Ten commands, one verified message, one backup. Everything else — groups,
scheduled sending, hiding your IP address, checking a host, using it from a
program — is in [README.md](README.md). If you are an agent rather than a
person, read [LLM.md](LLM.md) instead.

## When a command refuses

Every refusal prints three things: what failed, a short code like
`kusanagi.invite_spent`, and the line to run next. Do what the last line says.
Every code is listed in [docs/codes.md](docs/codes.md) with its fix.
