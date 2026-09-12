#!/usr/bin/env node
// The shell: picks the platform package npm installed alongside this one
// and execs its binary. No download, no second trust root beyond the
// registry the user already trusted with `npx`.
const { spawnSync } = require("node:child_process");
const path = require("node:path");

const table = {
  "linux-x64": ["@kusanagi/cli-linux-x64", "kusanagi"],
  "darwin-arm64": ["@kusanagi/cli-darwin-arm64", "kusanagi"],
  "win32-x64": ["@kusanagi/cli-win32-x64", "kusanagi.exe"],
};
const key = `${process.platform}-${process.arch}`;
const entry = table[key];
if (!entry) {
  console.error(
    `kusanagi has no binary for ${key}. ` +
      `See https://github.com/2youg1/kusanagi/releases for a build, ` +
      `or build from source: git clone https://github.com/2youg1/kusanagi`
  );
  process.exit(1);
}
let bin;
try {
  bin = require.resolve(`${entry[0]}/bin/${entry[1]}`);
} catch {
  console.error(
    `platform package ${entry[0]} is missing. ` +
      `Reinstall with optional dependencies enabled ` +
      `(npm install --include=optional), or fetch the release asset directly: ` +
      `https://github.com/2youg1/kusanagi/releases`
  );
  process.exit(1);
}
if (process.platform === "win32") {
  bin = path.normalize(bin);
}
const r = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
process.exit(r.status ?? 1);
