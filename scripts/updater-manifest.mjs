// Writes latest.json, the manifest the in-app updater reads from releases/latest/download/latest.json
// (docs/features/ui/auto-update.md). Run by the release workflow's publish job after every build has uploaded:
//   node scripts/updater-manifest.mjs <tag> <dir with the downloaded .sig files> <owner/repo> > latest.json
// Each installer's signature (<fixed name>.sig) becomes one platform entry; the updater picks
// "<os>-<arch>-<bundle type>" first, then "<os>-<arch>". A required signature that is missing fails the release.
import fs from "node:fs";
import path from "node:path";

const [tag, dir, repo] = process.argv.slice(2);
if (!tag || !dir || !repo) {
  console.error("usage: node scripts/updater-manifest.mjs <tag> <sig dir> <owner/repo>");
  process.exit(2);
}

// platform keys → the release file the updater downloads and installs; optional ones only if their .sig exists
const ENTRIES = [
  { keys: ["windows-x86_64-nsis", "windows-x86_64"], file: "Maestro-windows-x64-setup.exe" },
  { keys: ["windows-x86_64-msi"], file: "Maestro-windows-x64.msi" },
  { keys: ["darwin-aarch64-app", "darwin-aarch64"], file: "Maestro-macos-arm64.app.tar.gz" },
  { keys: ["darwin-x86_64-app", "darwin-x86_64"], file: "Maestro-macos-x64.app.tar.gz" },
  { keys: ["linux-x86_64-appimage", "linux-x86_64"], file: "Maestro-linux-x86_64.AppImage" },
  { keys: ["linux-x86_64-deb"], file: "Maestro-linux-amd64.deb", optional: true },
  { keys: ["linux-x86_64-rpm"], file: "Maestro-linux-x86_64.rpm", optional: true },
];

const platforms = {};
const missing = [];
for (const e of ENTRIES) {
  const sig = path.join(dir, `${e.file}.sig`);
  if (!fs.existsSync(sig)) {
    if (!e.optional) missing.push(`${e.file}.sig`);
    continue;
  }
  const entry = { signature: fs.readFileSync(sig, "utf8").trim(), url: `https://github.com/${repo}/releases/download/${tag}/${e.file}` };
  for (const k of e.keys) platforms[k] = entry;
}
if (missing.length) {
  console.error(`missing signatures: ${missing.join(", ")}`);
  process.exit(1);
}

const manifest = {
  version: tag.replace(/^v/, ""),
  notes: `https://github.com/${repo}/releases/tag/${tag}`,
  pub_date: new Date().toISOString(),
  platforms,
};
process.stdout.write(JSON.stringify(manifest, null, 2) + "\n");
