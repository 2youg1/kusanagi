# The npm channel

Four packages carry one release: the shell `@kasanagi/cli`, and one platform
package per target holding a single binary. `npm` and `bun` read `os`/`cpu` on
the platform packages and download only the one that matches, so an install
costs one binary and the shell never reaches the network.

```
npm/cli/                  @kasanagi/cli              bin.js, pins the three below
npm/platform-linux-x64/   @kasanagi/cli-linux-x64    bin/kusanagi
npm/platform-darwin-arm64/@kasanagi/cli-darwin-arm64 bin/kusanagi
npm/platform-win32-x64/   @kasanagi/cli-win32-x64    bin/kusanagi.exe
```

## Versions

Every version in these four `package.json` files is the literal token
`0.0.0-placeholder` — the shell's own version and its three pins included.
`scripts/npm-pack.sh` replaces every occurrence with the version the release tag
names, so the tag is the only place a release version is written.

The tag minus its leading `v` is the version, verbatim:

| tag | published as |
|---|---|
| `v0.0.1-Pre-alpha-260913` | `0.0.1-Pre-alpha-260913` |
| `v0.0.2` | `0.0.2` |

A tag outside `v<major>.<minor>.<patch>[-prerelease]` stops the pack script
rather than reaching the registry as a version nobody chose. `AGENTS.md` owns
the grammar a release tag actually uses, stage and date included. **Never hand-edit a
version in this directory.**

## A published version is never unpublished

The next version supersedes the last one; nothing is taken back. Unpublishing is
the one npm operation that can destroy a package name, and a name that ceases to
exist takes its trusted-publisher configuration with it, which breaks every
release afterwards. A version published by mistake is therefore superseded, not
retracted.

What makes that safe is the `latest` dist-tag, which `release.yml` moves on every
release: `npm install @kasanagi/cli` and `npx @kasanagi/cli` follow the tag, not
semver order, so the newest release is what an install gets even while an older
version remains in the registry.

One version does sort above every later one and will stay visible in
`npm view @kasanagi/cli versions`: `0.0.1-prealpha.0`, published under the tag
grammar this repository no longer uses. Prerelease identifiers compare as text,
so `0.0.1-Pre-alpha-260913` sorts below it. **Every tag after
`v0.0.1-Pre-alpha-260913` therefore starts at `v0.0.2`**, which puts the version
number itself above the residue and keeps the order monotone from there on.

## Publishing

The `npm` job in `release.yml` does it on every tag: it packs the built binaries,
publishes the three platform packages, waits until the registry serves each pin,
then publishes the shell. That order is not cosmetic — the shell declares the
platform packages as `optionalDependencies` at an exact version, and a pin the
registry cannot yet resolve is an install that fails on somebody else's machine.

Credentials are npm trusted publishing over OIDC: the job asks for
`id-token: write`, npm exchanges the token for a short-lived credential, and no
npm token is stored anywhere. It also earns each version a provenance
attestation, which a stored token would not.

## Bootstrapping a package name — once per name, by hand

**OIDC cannot create a package.** npm requires a package to exist before its
settings will accept a trusted publisher, so the first version of each of the
four names is published from a person's machine, and every version after that
comes from CI. Run this once, from a checkout at the tag:

```bash
npm login                       # as a member of the kasanagi org
gh release download v0.0.1-Pre-alpha-260913 --dir dist --pattern 'kusanagi-*'
bash scripts/npm-pack.sh v0.0.1-Pre-alpha-260913 dist out/npm
for dir in out/npm/platform-*; do npm publish "$dir" --tag latest --otp=CODE; done
npm publish out/npm/cli --tag latest --otp=CODE
```

Publishing asks for a one-time password, so `--otp` carries a code from the
authenticator on the account. Without it npm answers `EOTP` and publishes
nothing.

Then point each package at this workflow. `npm trust` does what the settings
page on npmjs.com does, so no browser is involved:

```bash
for package in @kasanagi/cli @kasanagi/cli-linux-x64 \
               @kasanagi/cli-darwin-arm64 @kasanagi/cli-win32-x64; do
  npm trust github "$package" --repo 2youg1/kusanagi --file release.yml \
    --allow-publish --yes
done
```

`--allow-publish` is not optional here. Without it a package accepts only
`npm stage publish`, while the release lane calls `npm publish` — the mistake
costs nothing today and fails every release afterwards. Check the result with
`npm trust list <package>`. After that the manual path is finished; tagging is
the whole release.

## Verifying a release

On each platform, from a machine that has never installed it:

```bash
npx --yes @kasanagi/cli@VERSION id
bunx --yes @kasanagi/cli@VERSION id
```

Both must print a handle. A silent exit with status 0 on Windows means `bin.js`
lost its shebang — npm's generated wrapper reads that line to decide what
interprets the file.
