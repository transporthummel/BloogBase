// Writes src-tauri/catalog/original-data.json: the SHA-256 of every data
// file (games.json `dataFiles`) of every known release, read from the
// owner's copies in .local/games/versions. Rerun when a version is added.
//
//   node scripts/original-data.mjs [versions folder]

import { createHash } from "node:crypto";
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/(\w:)/, "$1");
const versionsDir = process.argv[2] ?? join(root, ".local/games/versions");
const catalog = JSON.parse(readFileSync(join(root, "src-tauri/catalog/games.json"), "utf8"));
const sha = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");

const out = {
  $comment:
    "SHA-256 of every data file (games.json `dataFiles`) of every release in games.json, by game and file name. A known release whose data files are not among them holds a mod. Written by scripts/original-data.mjs from .local/games/versions.",
};
const seen = new Set();
for (const folder of readdirSync(versionsDir).sort()) {
  const dir = join(versionsDir, folder);
  if (!statSync(dir).isDirectory()) continue;
  const files = new Map(readdirSync(dir).map((f) => [f.toUpperCase(), join(dir, f)]));
  const hash = new Map();
  const hashOf = (name) => {
    if (!hash.has(name)) hash.set(name, sha(files.get(name)));
    return hash.get(name);
  };
  const match = catalog.games.flatMap((game) =>
    game.versions
      .filter((v) =>
        Object.entries(v.files).every(
          ([name, h]) => files.has(name.toUpperCase()) && hashOf(name.toUpperCase()) === h,
        ),
      )
      .map((v) => ({ game, version: v })),
  )[0];
  if (!match) throw new Error(`${folder}: no catalog version`);
  seen.add(match.version.id);
  const data = (out[match.game.id] ??= {});
  for (const name of match.game.dataFiles.flatMap((d) => d.split("|"))) {
    if (!files.has(name)) continue;
    const list = (data[name] ??= []);
    const h = hashOf(name);
    if (!list.includes(h)) list.push(h);
  }
}
const missing = catalog.games
  .flatMap((g) => g.versions.map((v) => v.id))
  .filter((id) => !seen.has(id));
if (missing.length) throw new Error(`no copy of ${missing.join(", ")}`);
for (const game of Object.keys(out).filter((k) => k !== "$comment")) {
  for (const list of Object.values(out[game])) list.sort();
}
writeFileSync(
  join(root, "src-tauri/catalog/original-data.json"),
  JSON.stringify(out, null, 2) + "\n",
);
