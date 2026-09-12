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

| tag | published as |
|---|---|
| `v0.0.2` | `0.0.2` |
| `v0.0.1prealpha` | `0.0.1-prealpha.0` |

A tag outside `v<major>.<minor>.<patch>[identifier]` stops the pack script
rather than reaching the registry as a version nobody chose. **Never hand-edit a
version in this directory.**

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
gh release download v0.0.1prealpha --dir dist --pattern 'kusanagi-*'
bash scripts/npm-pack.sh v0.0.1prealpha dist out/npm
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
