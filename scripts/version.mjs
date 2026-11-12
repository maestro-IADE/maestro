// The app's version lives in five files. `node scripts/version.mjs 0.2.0` sets it in all of them;
// `node scripts/version.mjs --check v0.2.0` fails unless all of them say 0.2.0 (the release workflow runs this).
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SEMVER = /^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/;

// each file: how to read the version and how to write a new one, keeping the rest of the file as it is
const FILES = {
  "package.json": {
    get: (s) => JSON.parse(s).version,
    set: (s, v) => s.replace(/("version":\s*")[^"]*(")/, `$1${v}$2`),
  },
  "package-lock.json": {
    get: (s) => { const j = JSON.parse(s); return j.version === j.packages[""].version ? j.version : `${j.version} / ${j.packages[""].version}`; },
    // the root "version" and packages[""].version: the first two "version" keys in the file
    set: (s, v) => { let n = 0; return s.replace(/("version":\s*")[^"]*(")/g, (m, a, b) => (n++ < 2 ? `${a}${v}${b}` : m)); },
  },
  "src-tauri/Cargo.toml": {
    get: (s) => s.match(/\[package\][^[]*?\nversion\s*=\s*"([^"]+)"/)[1],
    set: (s, v) => s.replace(/(\[package\][^[]*?\nversion\s*=\s*")[^"]+(")/, `$1${v}$2`),
  },
  "src-tauri/Cargo.lock": {
    get: (s) => s.match(/\nname = "maestro"\r?\nversion = "([^"]+)"/)[1],
    set: (s, v) => s.replace(/(\nname = "maestro"\r?\nversion = ")[^"]+(")/, `$1${v}$2`),
  },
  "src-tauri/tauri.conf.json": {
    get: (s) => JSON.parse(s).version,
    set: (s, v) => s.replace(/("version":\s*")[^"]*(")/, `$1${v}$2`),
  },
};

const read = (f) => fs.readFileSync(path.join(root, f), "utf8");
const [arg, value] = process.argv.slice(2);

if (arg === "--check") {
  const want = (value || "").replace(/^v/, "");
  const wrong = Object.entries(FILES).map(([f, x]) => [f, x.get(read(f))]).filter(([, v]) => v !== want);
  if (wrong.length) {
    for (const [f, v] of wrong) console.error(`${f}: ${v}, expected ${want}`);
    console.error(`run: node scripts/version.mjs ${want}`);
    process.exit(1);
  }
  console.log(`all files at ${want}`);
} else if (arg && SEMVER.test(arg)) {
  for (const [f, x] of Object.entries(FILES)) {
    const before = read(f);
    const after = x.set(before, arg);
    if (x.get(after) !== arg) throw new Error(`could not set the version in ${f}`);
    fs.writeFileSync(path.join(root, f), after);
    console.log(`${f}: ${x.get(before)} -> ${arg}`);
  }
} else {
  console.error("usage: node scripts/version.mjs <X.Y.Z> | --check <vX.Y.Z>");
  process.exit(2);
}
