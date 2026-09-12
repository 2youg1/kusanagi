# npm packages: `@kusanagi/cli` + three platform packages

The shell (`cli/`) carries `bin.js` and nothing else. Each platform package
carries one release binary under `bin/`. `npx @kusanagi/cli` picks the
platform package npm installed and runs its binary; the shell never touches
the network.

## Versions

`package.json` files carry `0.0.0-placeholder` (shell) and fixed
`optionalDependencies` pins. The `npm-versions` job in `release.yml` stamps
all four from the tag (`v0.0.1prealpha` → `0.0.1-prealpha.0`) and uploads the
stamped `npm/` dirs as the `npm-dirs` artifact. Never hand-edit a version.

## Publishing (by hand, after the release page shows all assets)

Nothing in CI publishes to the registry. After `npm-versions` is green:

```bash
# one-time: the scope must exist and you must be logged in
npm login
# from the downloaded npm-dirs artifact:
cd npm/platform-linux-x64 && npm publish --access public && cd ../..
cd npm/platform-darwin-arm64 && npm publish --access public && cd ../..
cd npm/platform-win32-x64 && npm publish --access public && cd ../..
# the platform binaries ride the release assets, so pack each dir with its
# binary copied from the release page under bin/ first
cd npm/cli && npm publish --access public
```

Verify with a cold install on each platform:

```bash
npx --yes @kusanagi/cli@VERSION id
bunx --yes @kusanagi/cli@VERSION id
```

Both must print a handle. Until they do, the npm issue stays open.
