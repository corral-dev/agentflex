#!/usr/bin/env node
// Assembles publishable npm packages from prebuilt agentflex binaries.
//
// Usage:
//   node scripts/build-npm.mjs [--version X.Y.Z] [--bin-dir DIR]
//                               [--out-dir npm/dist]
//                               [--main-pkg npm/agentflex]
//                               [--platform-tmpl npm/platform/package.json.tmpl]
//
// --version defaults to the `[package] version` in Cargo.toml.
// --bin-dir (default: `bins/`) holds built binaries named by Rust target:
//   aarch64-apple-darwin, x86_64-apple-darwin,
//   aarch64-unknown-linux-musl, x86_64-unknown-linux-musl,
//   x86_64-pc-windows-msvc.exe, aarch64-pc-windows-msvc.exe
// Only targets with a binary present get a platform package; at least one
// recognized binary is required.
//
// Output (all under --out-dir, default `npm/dist/`, git-ignored):
//   <out>/agentflex/                     main package (launcher + README)
//   <out>/agentflex-<os>-<arch>/         one platform package per binary
//     package.json + bin/agentflex[.exe]
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..");

const VERSION_PLACEHOLDER = "0.0.0-dev";

// Rust target -> npm platform package mapping.
const TARGETS = [
  { target: "aarch64-apple-darwin", pkg: "agentflex-darwin-arm64", os: "darwin", cpu: "arm64", bin: "agentflex" },
  { target: "x86_64-apple-darwin", pkg: "agentflex-darwin-x64", os: "darwin", cpu: "x64", bin: "agentflex" },
  { target: "aarch64-unknown-linux-musl", pkg: "agentflex-linux-arm64", os: "linux", cpu: "arm64", bin: "agentflex" },
  { target: "x86_64-unknown-linux-musl", pkg: "agentflex-linux-x64", os: "linux", cpu: "x64", bin: "agentflex" },
  { target: "aarch64-pc-windows-msvc", pkg: "agentflex-win32-arm64", os: "win32", cpu: "arm64", bin: "agentflex.exe" },
  { target: "x86_64-pc-windows-msvc", pkg: "agentflex-win32-x64", os: "win32", cpu: "x64", bin: "agentflex.exe" },
];

const USAGE = `usage: node scripts/build-npm.mjs [--version X.Y.Z] [--bin-dir DIR] [--out-dir npm/dist]
       [--main-pkg npm/agentflex] [--platform-tmpl npm/platform/package.json.tmpl]

  --version   npm version to stamp (default: [package] version from Cargo.toml)
  --bin-dir   directory of built binaries named by Rust target
              (default: bins/, relative to the repository root)
  --help      print this message and exit`;

function parseArgs(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") {
      opts.help = true;
    } else if (arg === "--version" || arg === "--bin-dir" || arg === "--out-dir" || arg === "--main-pkg" || arg === "--platform-tmpl") {
      const value = argv[i + 1];
      if (value === undefined || value.startsWith("--")) {
        throw new Error(`missing value for ${arg}`);
      }
      opts[arg.slice(2)] = value;
      i++;
    } else if (arg.startsWith("--")) {
      throw new Error(`unknown flag ${arg}\n${USAGE}`);
    } else {
      throw new Error(`unexpected argument ${arg}\n${USAGE}`);
    }
  }
  return opts;
}

function readCargoVersion() {
  const cargoToml = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!match) {
    throw new Error("cannot find [package] version in Cargo.toml");
  }
  return match[1];
}

function checkVersion(version) {
  if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
    throw new Error(`version "${version}" is not semver-shaped (npm requires x.y.z)`);
  }
}

// Locate a binary file by exact name, searching binDir top level first,
// then one level of subdirectories (covers both flat CI downloads and
// per-target artifact layouts).
function findBinary(binDir, name) {
  const direct = join(binDir, name);
  if (existsSync(direct) && statSync(direct).isFile()) {
    return direct;
  }
  for (const entry of readdirSync(binDir)) {
    const nested = join(binDir, entry, name);
    try {
      if (statSync(nested).isFile()) {
        return nested;
      }
    } catch {
      // Not a directory or unreadable; ignore.
    }
  }
  return undefined;
}

function main() {
  let opts;
  try {
    opts = parseArgs(process.argv.slice(2));
  } catch (error) {
    process.stderr.write(`build-npm: ${error.message}\n`);
    process.exit(1);
  }
  if (opts.help) {
    console.log(USAGE);
    return;
  }

  const version = opts.version || readCargoVersion();
  checkVersion(version);
  const binDir = resolve(ROOT, opts.bin || opts["bin-dir"] || "bins");
  const outDir = resolve(ROOT, opts["out-dir"] || join("npm", "dist"));
  const mainPkgDir = resolve(ROOT, opts["main-pkg"] || join("npm", "agentflex"));
  const platformTmpl = resolve(ROOT, opts["platform-tmpl"] || join("npm", "platform", "package.json.tmpl"));

  if (!existsSync(binDir) || !statSync(binDir).isDirectory()) {
    process.stderr.write(`build-npm: bin directory not found: ${binDir}\n`);
    process.exit(1);
  }

  const present = [];
  for (const t of TARGETS) {
    const asset = t.target + (t.bin.endsWith(".exe") ? ".exe" : "");
    const found = findBinary(binDir, asset);
    if (found) {
      present.push({ ...t, source: found });
    } else {
      console.log(`build-npm: no binary for ${t.target}, skipping ${t.pkg}`);
    }
  }
  if (present.length === 0) {
    process.stderr.write(`build-npm: no recognized binaries in ${binDir} (expected files named by Rust target, e.g. aarch64-apple-darwin)\n`);
    process.exit(1);
  }

  rmSync(outDir, { recursive: true, force: true });

  // Main package: template package.json with the placeholder version stamped
  // (name + optionalDependencies), plus launcher and README copied verbatim.
  const mainOut = join(outDir, "agentflex");
  mkdirSync(join(mainOut, "bin"), { recursive: true });
  const mainPkg = readFileSync(join(mainPkgDir, "package.json"), "utf8");
  if (!mainPkg.includes(VERSION_PLACEHOLDER)) {
    process.stderr.write(`build-npm: placeholder ${VERSION_PLACEHOLDER} missing from ${join(mainPkgDir, "package.json")}\n`);
    process.exit(1);
  }
  writeFileSync(join(mainOut, "package.json"), mainPkg.split(VERSION_PLACEHOLDER).join(version));
  copyFileSync(join(mainPkgDir, "bin", "agentflex.js"), join(mainOut, "bin", "agentflex.js"));
  copyFileSync(join(mainPkgDir, "README.md"), join(mainOut, "README.md"));

  // Platform packages: one per binary present.
  const tmpl = readFileSync(platformTmpl, "utf8");
  for (const key of ["__NAME__", "__VERSION__", "__OS__", "__CPU__"]) {
    if (!tmpl.includes(key)) {
      process.stderr.write(`build-npm: placeholder ${key} missing from ${platformTmpl}\n`);
      process.exit(1);
    }
  }
  for (const t of present) {
    const pkgOut = join(outDir, t.pkg);
    mkdirSync(join(pkgOut, "bin"), { recursive: true });
    const pkgJson = tmpl
      .split("__NAME__").join(t.pkg)
      .split("__VERSION__").join(version)
      .split("__OS__").join(t.os)
      .split("__CPU__").join(t.cpu);
    JSON.parse(pkgJson); // fail fast on a broken template
    writeFileSync(join(pkgOut, "package.json"), pkgJson.endsWith("\n") ? pkgJson : pkgJson + "\n");
    const dest = join(pkgOut, "bin", t.bin);
    copyFileSync(t.source, dest);
    if (statSync(dest).size === 0) {
      process.stderr.write(`build-npm: refusing to ship empty binary for ${t.pkg}\n`);
      process.exit(1);
    }
    if (!t.bin.endsWith(".exe")) {
      chmodSync(dest, 0o755);
    }
  }

  console.log(`build-npm: version ${version}`);
  console.log(`build-npm: wrote ${join(outDir, "agentflex")}`);
  for (const t of present) {
    console.log(`build-npm: wrote ${join(outDir, t.pkg)} (from ${t.source})`);
  }
}

main();
