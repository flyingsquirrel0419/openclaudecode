#!/usr/bin/env node
const { chmodSync, copyFileSync, existsSync, writeFileSync } = require("node:fs");
const https = require("node:https");
const { join, resolve } = require("node:path");

const target = process.platform === "win32" ? "occ.exe" : "occ";
const source = resolve(__dirname, "..", "..", "target", "release", target);
const dest = join(__dirname, target);
const pkg = require("../package.json");

if (existsSync(source)) {
  copyFileSync(source, dest);
  if (process.platform !== "win32") chmodSync(dest, 0o755);
  console.log(`claude-occ: installed local ${target}`);
} else {
  const asset = assetName();
  const repo = process.env.CLAUDE_OCC_RELEASE_REPO || "flyingsquirrel0419/claude-occ";
  const url = `https://github.com/${repo}/releases/download/v${pkg.version}/${asset}`;
  download(url, dest).then(() => {
    if (process.platform !== "win32") chmodSync(dest, 0o755);
    console.log(`claude-occ: installed ${asset}`);
  }).catch((err) => {
    console.warn(`claude-occ: could not download ${asset}: ${err.message}`);
    console.warn("claude-occ: run `cargo build --release` and copy target/release/occ into npm/bin for local packaging.");
    process.exitCode = 1;
  });
}

function assetName() {
  const platform = {
    darwin: "apple-darwin",
    linux: "unknown-linux-gnu",
    win32: "pc-windows-msvc",
  }[process.platform];
  const arch = {
    x64: "x86_64",
    arm64: "aarch64",
  }[process.arch];
  if (!platform || !arch) {
    throw new Error(`unsupported platform ${process.platform}/${process.arch}`);
  }
  return `occ-${arch}-${platform}${process.platform === "win32" ? ".exe" : ""}`;
}

function download(url, path) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        download(res.headers.location, path).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error(`HTTP ${res.statusCode}`));
        res.resume();
        return;
      }
      const chunks = [];
      res.on("data", (chunk) => chunks.push(chunk));
      res.on("end", () => {
        writeFileSync(path, Buffer.concat(chunks));
        resolve();
      });
    }).on("error", reject);
  });
}
