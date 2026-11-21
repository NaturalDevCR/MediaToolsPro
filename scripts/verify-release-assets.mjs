import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const folder = process.argv[2];
assert.ok(
  folder,
  "Pass the directory containing the downloaded release assets.",
);
const { version } = JSON.parse(readFileSync("package.json", "utf8"));
const files = readdirSync(folder)
  .filter((name) => name !== "SHA256SUMS.txt")
  .sort();
const required = [
  [
    "macOS Apple Silicon DMG",
    (name) => /(?:aarch64|arm64).*\.dmg$/i.test(name),
  ],
  ["macOS Intel DMG", (name) => /(?:x64|x86_64).*\.dmg$/i.test(name)],
  ["Windows x64 EXE", (name) => /x64.*\.exe$/i.test(name)],
  ["Windows x64 MSI", (name) => /x64.*\.msi$/i.test(name)],
  ["Linux x64 AppImage", (name) => /(?:amd64|x86_64).*\.AppImage$/i.test(name)],
  ["Linux x64 DEB", (name) => /amd64.*\.deb$/i.test(name)],
  ["Linux x64 RPM", (name) => /x86_64.*\.rpm$/i.test(name)],
];
for (const [label, matches] of required) {
  const file = files.find((name) => name.includes(version) && matches(name));
  assert.ok(file, `Missing installer: ${label}`);
  assert.ok(
    statSync(join(folder, file)).size > 100_000,
    `Installer is empty or incomplete: ${file}`,
  );
  console.log(`Verified: ${label} (${file})`);
}
const sums = files
  .map(
    (name) =>
      `${createHash("sha256")
        .update(readFileSync(join(folder, name)))
        .digest("hex")}  ${name}`,
  )
  .join("\n");
writeFileSync(join(folder, "SHA256SUMS.txt"), `${sums}\n`);
console.log(`SHA-256 checksums generated for ${files.length} assets.`);
