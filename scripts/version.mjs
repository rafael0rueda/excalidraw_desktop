// Every place the app's version is written down, and the one command that
// changes them together.
//
//   node scripts/version.mjs            # print what each place says
//   node scripts/version.mjs 0.5.4      # set all of them (npm run bump -- 0.5.4)
//
// A bump used to be four files edited by hand and a follow-up commit for the
// README, and `package-lock.json` was never one of the four: it sat at 0.5.0
// for three releases. `scripts/check.mjs` imports `versions()` and fails when
// the places disagree, so a hand edit that misses one is caught by the gate.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const VERSION = String.raw`\d+\.\d+\.\d+`;

// Each place is a file and the patterns that find the version in it. Group 1
// is what comes before the version, so a replacement can put it back.
const PLACES = [
  ["package.json", [new RegExp(`^(\\{\\s*"name": "excalidraw-desktop",[^}]*?"version": ")${VERSION}`)]],
  [
    "package-lock.json",
    [
      new RegExp(`^(\\{\\s*"name": "excalidraw-desktop",\\s*"version": ")${VERSION}`),
      new RegExp(`("packages": \\{\\s*"": \\{\\s*"name": "excalidraw-desktop",\\s*"version": ")${VERSION}`),
    ],
  ],
  ["src-tauri/tauri.conf.json", [new RegExp(`("productName": "Excalidraw Desktop",\\s*"version": ")${VERSION}`)]],
  ["src-tauri/Cargo.toml", [new RegExp(`(\\[package\\]\\nname = "excalidraw-desktop"\\nversion = ")${VERSION}`)]],
  ["src-tauri/Cargo.lock", [new RegExp(`(name = "excalidraw-desktop"\\nversion = ")${VERSION}`)]],
  [
    "README.md",
    [
      new RegExp(`(The commands name version )${VERSION}`, "g"),
      new RegExp(`(Excalidraw Desktop-)${VERSION}(?=-1\\.x86_64)`, "g"),
      new RegExp(`(Excalidraw Desktop_)${VERSION}(?=_amd64\\.deb)`, "g"),
    ],
  ],
];

/** `[place, version]` for every occurrence. A pattern that finds nothing is an error. */
export function versions(root = ROOT) {
  const found = [];
  for (const [file, patterns] of PLACES) {
    const text = readFileSync(join(root, file), "utf8");
    for (const pattern of patterns) {
      const matches = [...text.matchAll(new RegExp(pattern.source, "g"))];
      if (!matches.length) throw new Error(`${file}: no version found by ${pattern.source}`);
      for (const match of matches) found.push([file, match[0].slice(match[1].length)]);
    }
  }
  return found;
}

function bump(next, root = ROOT) {
  if (!new RegExp(`^${VERSION}$`).test(next)) throw new Error(`not a version: ${next}`);
  for (const [file, patterns] of PLACES) {
    const path = join(root, file);
    let text = readFileSync(path, "utf8");
    for (const pattern of patterns) {
      text = text.replace(new RegExp(pattern.source, "g"), (_whole, before) => `${before}${next}`);
    }
    writeFileSync(path, text);
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const next = process.argv[2];
  if (next) bump(next);
  const found = versions();
  for (const [file, version] of found) console.log(`${version}  ${file}`);
  if (new Set(found.map(([, version]) => version)).size > 1) {
    console.error("\nThese disagree. Run: node scripts/version.mjs <version>");
    process.exit(1);
  }
  if (next) console.log(`\nNow commit, and tag it: git tag v${next}`);
}
