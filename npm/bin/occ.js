#!/usr/bin/env node
const { spawnSync } = require("node:child_process");
const { existsSync } = require("node:fs");
const { join } = require("node:path");

const exe = process.platform === "win32" ? "occ.exe" : "occ";
const bin = join(__dirname, exe);

if (!existsSync(bin)) {
  console.error("claude-occ: missing platform binary. Reinstall the package or build from source.");
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`claude-occ: failed to launch occ: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status ?? 0);
