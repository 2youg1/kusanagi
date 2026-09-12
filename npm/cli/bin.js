#!/usr/bin/env node
// The shell: picks the platform package npm installed alongside this one and
// runs its binary. Nothing here reaches the network, so `npx` adds no trust
// root beyond the registry the user already reached for.
//
// The shebang above is load-bearing. The wrapper npm generates reads it to
// decide what interprets this file; without it, Windows runs the file as a
// shell script, which prints nothing and exits 0.
const { spawnSync } = require("node:child_process");
const { constants } = require("node:os");

const binaries = {
  "linux-x64": ["@kasanagi/cli-linux-x64", "kusanagi"],
  "darwin-arm64": ["@kasanagi/cli-darwin-arm64", "kusanagi"],
  "win32-x64": ["@kasanagi/cli-win32-x64", "kusanagi.exe"],
};
const platform = `${process.platform}-${process.arch}`;
const entry = binaries[platform];
if (!entry) {
  console.error(
    `kusanagi publishes no binary for ${platform}. ` +
      `Build it from source instead: https://github.com/2youg1/kusanagi#install`
  );
  process.exit(1);
}
const [package_, binary] = entry;

let executable;
try {
  executable = require.resolve(`${package_}/bin/${binary}`);
} catch {
  console.error(
    `the platform package ${package_} is not installed. ` +
      `Reinstall with optional dependencies enabled ` +
      `(npm install --include=optional), or take the binary from ` +
      `https://github.com/2youg1/kusanagi/releases`
  );
  process.exit(1);
}

const run = spawnSync(executable, process.argv.slice(2), { stdio: "inherit" });
if (run.error) {
  console.error(`kusanagi could not start ${executable}: ${run.error.message}`);
  process.exit(1);
}
// A process killed by a signal has no exit status. Report it the way a shell
// does, so a caller reads the same number whether it ran the binary directly
// or went through this shell.
if (run.signal) {
  process.exit(128 + (constants.signals[run.signal] ?? 0));
}
process.exit(run.status ?? 1);
