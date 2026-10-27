// Prepares a release, run by .github/workflows/release.yml (docs/releasing.md):
// writes the version into package.json, Cargo.toml, Cargo.lock and
// tauri.conf.json, moves CHANGELOG.md's "Unreleased" section and the app's
// unreleased news (src/updates/changelog.json) under the version, and writes
// the new CHANGELOG.md section to a file for the release's description.
//
//   node scripts/release.mjs <version> <date> <notes file> [<last released version>]
//
// Changes nothing when something is missing or the version isn't newer.
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const CATEGORIES = ["Features", "Improvements", "Bug Fixes", "Misc"];
const UNRELEASED = "## Unreleased";

const [version, date, notesFile, last] = process.argv.slice(2);

function fail(message) {
  console.error(`::error::${message}`);
  process.exit(1);
}

/** Compares `1.10.0` with `1.9.2` by number. */
function compare(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return pa[i] - pb[i];
  return 0;
}

if (!/^\d+\.\d+\.\d+$/.test(version ?? "")) fail(`"${version}" isn't a version like 1.2.3.`);
if (!/^\d{4}-\d{2}-\d{2}$/.test(date ?? "")) fail(`"${date}" isn't a date like 2026-10-27.`);
if (!notesFile) fail("No file for the release notes given.");
if (last && compare(version, last) <= 0) fail(`${version} isn't newer than ${last}.`);

const read = (file) => readFileSync(join(root, file), "utf8");
// The first group of each pattern is what comes before the version, the
// second the version.
const versions = {
  "package.json": /^(\{\n(?:.*\n)*?  "version": ")([^"]+)/,
  "src-tauri/tauri.conf.json": /^(\{\n(?:.*\n)*?  "version": ")([^"]+)/,
  "src-tauri/Cargo.toml": /^(\[package\]\n(?:[^[\n].*\n)*?version = ")([^"]+)/m,
  "src-tauri/Cargo.lock": /^(name = "bloogbase"\nversion = ")([^"]+)/m,
};
const written = {};
for (const [file, pattern] of Object.entries(versions)) {
  const text = read(file).replaceAll("\r\n", "\n");
  const current = text.match(pattern)?.[2];
  if (!current) fail(`No version found in ${file}.`);
  if (compare(version, current) < 0) {
    fail(`${version} is older than the version in ${file} (${current}).`);
  }
  written[file] = text.replace(pattern, `$1${version}`);
}

// CHANGELOG.md: the categories under "Unreleased" that have entries.
const changelog = read("CHANGELOG.md").replaceAll("\r\n", "\n");
const start = changelog.indexOf(`${UNRELEASED}\n`);
if (start < 0) fail(`CHANGELOG.md has no "${UNRELEASED}" section.`);
const bodyStart = start + UNRELEASED.length + 1;
const next = changelog.indexOf("\n## ", bodyStart);
const end = next < 0 ? changelog.length : next + 1;
const sections = changelog
  .slice(bodyStart, end)
  .split(/^(?=### )/m)
  .map((s) => s.trim())
  .filter((s) => s.startsWith("### ") && s.includes("\n"));
if (sections.length === 0) fail(`CHANGELOG.md's "${UNRELEASED}" section is empty.`);
const notes = `${sections.join("\n\n")}\n`;
const empty = CATEGORIES.map((c) => `### ${c}`).join("\n\n");
written["CHANGELOG.md"] =
  changelog.slice(0, start) +
  `${UNRELEASED}\n\n${empty}\n\n## ${version} (${date})\n\n${notes}\n` +
  changelog.slice(end);

// The app's news: what players see after updating, in both languages.
const news = JSON.parse(read("src/updates/changelog.json"));
const { en = [], de = [] } = news.unreleased ?? {};
if (en.length === 0 || de.length === 0) {
  fail('src/updates/changelog.json needs "unreleased" entries in English and German.');
}
news.releases = [{ version, en, de }, ...(news.releases ?? [])];
news.unreleased = { en: [], de: [] };
written["src/updates/changelog.json"] = `${JSON.stringify(news, null, 2)}\n`;

for (const [file, text] of Object.entries(written)) writeFileSync(join(root, file), text);
writeFileSync(notesFile, notes);
console.log(`Prepared ${version}:\n\n${notes}`);
