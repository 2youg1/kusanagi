# kusanagi

Unlinkable dead drops on a host nobody has to trust. A sender leaves a segment
at an opaque address, a reader collects it later, and the host that holds the
bytes learns neither who wrote it nor who read it.

Full documentation: <https://github.com/2youg1/kusanagi>

## Run it

```bash
npx @kasanagi/cli id
bunx @kasanagi/cli id
```

## Install the command

```bash
npm install --global @kasanagi/cli
bun install --global @kasanagi/cli
kusanagi id
```

Either form gives you the command `kusanagi`.

## What this package is

A shell of one JavaScript file. It picks the platform package npm installed
beside it and runs the binary inside it. The binaries are the same files the
[release page](https://github.com/2youg1/kusanagi/releases) carries, checked
against the release checksums when the package was built. Installing downloads
one platform package and no others; running it reaches the network only where
kusanagi itself does.

Published from GitHub Actions through npm trusted publishing, so every version
carries a provenance attestation linking it to the workflow run that built it.

Platforms: Linux x86-64, macOS arm64, Windows x86-64. On anything else, build
from source — the repository README has the three commands.

MPL-2.0.
