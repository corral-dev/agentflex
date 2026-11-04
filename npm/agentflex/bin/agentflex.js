#!/usr/bin/env node
// agentflex npm launcher.
//
// The real binary lives in a per-platform optional dependency
// (agentflex-darwin-arm64, agentflex-linux-x64, ...). This script resolves
// the matching package at *run* time and spawns the binary, so no
// postinstall step is needed (pnpm 10 and Bun block postinstall scripts by
// default, which is why install-time downloading is avoided).
//
// Reused pattern: one main package plus per-platform optional packages with
// `os`/`cpu` constraints, as in `@biomejs/biome` (CJS launcher resolving
// `<pkg>/biome[.exe]` and spawning it with stdio "inherit") and `ccusage`
// (explicit unsupported-platform error, exit-code forwarding, re-raising the
// child's termination signal, repairing the run-time executable bit). Like
// Biome the launcher is CJS with `spawnSync`, so `require.resolve` works
// verbatim on npm, pnpm and Bun; like ccusage a killed child re-raises the
// same signal on this process instead of silently mapping it to exit 0.
"use strict";

const { spawnSync } = require("node:child_process");
const { chmodSync, statSync } = require("node:fs");

// Test-only overrides (documented for launcher verification, e.g.
// `AGENTFLEX_PLATFORM=freebsd AGENTFLEX_ARCH=x64 npx agentflex` must print
// the unsupported-platform line below and exit 1).
const OVERRIDE_PLATFORM = process.env.AGENTFLEX_PLATFORM || undefined;
const OVERRIDE_ARCH = process.env.AGENTFLEX_ARCH || undefined;

// platform -> arch -> { package, file }
const PLATFORMS = {
  darwin: {
    arm64: { package: "agentflex-darwin-arm64", file: "bin/agentflex" },
    x64: { package: "agentflex-darwin-x64", file: "bin/agentflex" },
  },
  linux: {
    arm64: { package: "agentflex-linux-arm64", file: "bin/agentflex" },
    x64: { package: "agentflex-linux-x64", file: "bin/agentflex" },
  },
  win32: {
    arm64: { package: "agentflex-windows-arm64", file: "bin/agentflex.exe" },
    x64: { package: "agentflex-windows-x64", file: "bin/agentflex.exe" },
  },
};

const SUPPORTED = "darwin (arm64, x64), linux (x64, arm64), win32 (x64, arm64)";

function currentTarget() {
  return {
    platform: OVERRIDE_PLATFORM || process.platform,
    arch: OVERRIDE_ARCH || process.arch,
  };
}

// Every failure is one clear English line on stderr, exit code 1.
function fail(message) {
  process.stderr.write(`agentflex: ${message}\n`);
  process.exit(1);
}

function ensureExecutable(path, platform) {
  if (platform === "win32") {
    return;
  }
  try {
    if ((statSync(path).mode & 0o111) !== 0) {
      return;
    }
    chmodSync(path, 0o755);
  } catch {
    // Leave the error to spawn reporting below.
  }
}

function main() {
  const { platform, arch } = currentTarget();
  const target = `${platform}-${arch}`;
  const entry = Object.prototype.hasOwnProperty.call(PLATFORMS, platform)
    ? PLATFORMS[platform][arch]
    : undefined;
  if (!entry) {
    fail(`unsupported platform ${target}; supported platforms: ${SUPPORTED}.`);
  }
  let binary;
  try {
    binary = require.resolve(`${entry.package}/${entry.file}`);
  } catch {
    fail(
      `package "${entry.package}" for ${target} is not installed (reinstall agentflex with optional dependencies enabled); supported platforms: ${SUPPORTED}.`
    );
  }
  ensureExecutable(binary, platform);
  const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
  if (result.error) {
    fail(`cannot run "${entry.package}" for ${target}: ${result.error.message}.`);
  }
  if (result.signal) {
    // Re-raise so callers observe the same termination signal.
    try {
      process.kill(process.pid, result.signal);
    } catch {
      // Fall through to exit(1) when re-raising is impossible.
    }
    process.exit(1);
  }
  process.exit(result.status === null || result.status === undefined ? 1 : result.status);
}

main();
