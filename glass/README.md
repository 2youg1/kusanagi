# Glass

A native-rendered Native SDK app: the view lives in `src/app.native`
(declarative markup) and the logic in `src/main.zig` (`Model`, `Msg`,
`update`). No WebView, no npm, no build files — the `native` CLI owns
the build.

## Commands

```sh
native dev     # build and run the app with hot reload
native test    # run the app's test suite
native build   # produce a ReleaseFast binary in zig-out/bin/
native check   # validate src/*.native markup and app.json
```

## The binary beside it

Glass shells out to a `kusanagi` binary sitting next to `glass.exe` in
`zig-out/bin/` — it is how invitations are minted and reads are run. That
copy must be the current build (`cp ../target/release/kusanagi.exe
zig-out/bin/` after any Rust change, then restart Glass): an older binary
writes records a newer one refuses to read, and the window says so instead
of working.

## Hot reload

`src/app.native` is watched while `native dev` runs: edit it and the
window updates within ~2s without losing model state. Parse failures
keep the last good view.

## Owning the build

Need custom build logic? `native eject` writes a build.zig and
build.zig.zon into the app — from then on the `native` verbs drive
your files through `zig build` and never regenerate them.
