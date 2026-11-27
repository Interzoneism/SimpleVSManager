#!/usr/bin/env node
// Keeps the app version the same in package.json, Cargo.toml (workspace) and Cargo.lock.
// src-tauri/tauri.conf.json has no "version" on purpose: Tauri then takes it from Cargo.toml.
//
//   node scripts/version.mjs                 print the current version
//   node scripts/version.mjs 3.1.0           set the version everywhere
//   node scripts/version.mjs --check [tag]   fail if the files disagree (or don't match tag, e.g. v3.1.0)
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const file = (p) => join(root, p);
const read = (p) => readFileSync(file(p), "utf8");
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;
const CRATES = ["simple-vs-manager", "svsm-core"];

const cargoRe = /(\[workspace\.package\][^[]*?\nversion\s*=\s*")([^"]+)(")/;
// \r?\n: Windows checkouts may have CRLF line endings.
const lockRe = (name) => new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = ")([^"]+)(")`);

function current() {
  const pkg = JSON.parse(read("package.json")).version;
  const cargo = read("Cargo.toml").match(cargoRe)?.[2];
  const lock = Object.fromEntries(CRATES.map((c) => [c, read("Cargo.lock").match(lockRe(c))?.[2]]));
  const tauri = JSON.parse(read("src-tauri/tauri.conf.json")).version;
  return { pkg, cargo, lock, tauri };
}

function check(tag) {
  const v = current();
  const errors = [];
  if (!v.pkg || !SEMVER.test(v.pkg)) errors.push(`package.json version "${v.pkg}" is not semver`);
  if (v.cargo !== v.pkg) errors.push(`Cargo.toml has ${v.cargo}, package.json has ${v.pkg}`);
  for (const [c, lv] of Object.entries(v.lock)) {
    if (lv !== v.pkg) errors.push(`Cargo.lock has ${c} ${lv}, package.json has ${v.pkg}`);
  }
  if (v.tauri !== undefined && v.tauri !== v.pkg) {
    errors.push(`tauri.conf.json has version ${v.tauri}; remove it so Tauri uses Cargo.toml`);
  }
  if (tag && tag !== `v${v.pkg}`) errors.push(`tag ${tag} does not match version v${v.pkg}`);
  if (errors.length) {
    console.error(errors.map((e) => `version check: ${e}`).join("\n"));
    console.error("Fix with: node scripts/version.mjs <version>");
    process.exit(1);
  }
  console.log(v.pkg);
}

function set(next) {
  if (!SEMVER.test(next)) {
    console.error(`"${next}" is not a version like 3.1.0 or 3.1.0-beta.1`);
    process.exit(1);
  }
  const pkgText = read("package.json");
  writeFileSync(file("package.json"), pkgText.replace(/("version"\s*:\s*")[^"]+(")/, `$1${next}$2`));
  writeFileSync(file("Cargo.toml"), read("Cargo.toml").replace(cargoRe, `$1${next}$3`));
  let lock = read("Cargo.lock");
  for (const c of CRATES) lock = lock.replace(lockRe(c), `$1${next}$3`);
  writeFileSync(file("Cargo.lock"), lock);
  check();
}

const [arg, tag] = process.argv.slice(2);
if (!arg) console.log(current().pkg);
else if (arg === "--check") check(tag);
else set(arg.replace(/^v/, ""));
