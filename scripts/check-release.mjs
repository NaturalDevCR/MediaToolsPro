import assert from "node:assert/strict";
import { appendFileSync, readFileSync } from "node:fs";

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
const lock = readFileSync("src-tauri/Cargo.lock", "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const lockVersion = lock.match(
  /name = "mediatoolspro"\r?\nversion = "([^"]+)"/,
)?.[1];
const tag = process.argv[2] || `app-v${pkg.version}`;
assert.match(pkg.version, /^\d+\.\d+\.\d+$/, "Use a stable semantic version.");
assert.equal(
  config.version,
  pkg.version,
  "Tauri and package.json versions differ.",
);
assert.equal(
  cargoVersion,
  pkg.version,
  "Cargo.toml and package.json versions differ.",
);
assert.equal(
  lockVersion,
  pkg.version,
  "Cargo.lock and package.json versions differ.",
);
assert.equal(
  tag,
  `app-v${pkg.version}`,
  "Tag does not match the application version.",
);
const notes = readFileSync(`.github/release-notes/${pkg.version}.md`, "utf8");
assert.ok(
  notes.trim().length > 200,
  "Release notes are missing or incomplete.",
);
if (process.env.GITHUB_OUTPUT) {
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `version=${pkg.version}\ntag=${tag}\n`,
  );
}
console.log(`Release verified: ${tag}; all four version files agree.`);
