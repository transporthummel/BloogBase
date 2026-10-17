// Collects the licenses of everything shipped inside the app (Rust crates
// and npm packages, without dev dependencies) into
// public/third-party-licenses.txt, which the About page shows.
// Run before release builds: `pnpm licenses:generate`.
import { execFileSync, execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const manifest = join(root, "src-tauri", "Cargo.toml");
const OWN = new Set(["bloogbase", "keen-saves", "keen-graphics"]);
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense)/i;

const options = { cwd: root, encoding: "utf8", maxBuffer: 256 << 20 };
// pnpm is a .cmd script on Windows, which only runs in a shell; the
// arguments are fixed, so that's safe.
const run = (cmd, args) =>
  cmd === "pnpm" && process.platform === "win32"
    ? execSync(`pnpm ${args.join(" ")}`, options)
    : execFileSync(cmd, args, options);

/** Texts of a package's license files, read from its folder. */
function licenseTexts(dir) {
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return [];
  }
  return entries
    .filter((e) => e.isFile() && LICENSE_FILE.test(e.name))
    .map((e) => e.name)
    .sort()
    .map((n) => readFileSync(join(dir, n), "utf8").replace(/\r\n/g, "\n").trim());
}

/** @type {{ name: string; version: string; license: string; texts: string[] }[]} */
const components = [];

// Rust: only crates that are compiled into the app, for any target OS.
const shipped = new Set(
  run("cargo", [
    "tree",
    "--manifest-path",
    manifest,
    "-e",
    "normal",
    "--prefix",
    "none",
    "--target",
    "all",
    "-f",
    "{p}",
  ])
    .split("\n")
    .map((l) => l.replace(" (*)", "").trim())
    .filter(Boolean)
    .map((l) => l.split(" ").slice(0, 2).join(" ")),
);
const metadata = JSON.parse(
  run("cargo", ["metadata", "--manifest-path", manifest, "--format-version", "1"]),
);
for (const p of metadata.packages) {
  if (OWN.has(p.name) || !shipped.has(`${p.name} v${p.version}`)) continue;
  components.push({
    name: p.name,
    version: p.version,
    license: p.license ?? "see text",
    texts: licenseTexts(dirname(p.manifest_path)),
  });
}

// npm: production dependencies, bundled into the frontend.
const npm = JSON.parse(run("pnpm", ["licenses", "list", "--prod", "--json"]));
for (const [license, packages] of Object.entries(npm)) {
  for (const p of packages) {
    p.versions.forEach((version, i) =>
      components.push({ name: p.name, version, license, texts: licenseTexts(p.paths[i]) }),
    );
  }
}

// One copy of each distinct text, with the components that use it.
const byText = new Map();
const withoutText = [];
for (const c of components) {
  if (c.texts.length === 0) {
    withoutText.push(c);
    continue;
  }
  const text = c.texts.join("\n\n-----\n\n");
  const key = createHash("sha256").update(text).digest("hex");
  if (!byText.has(key)) byText.set(key, { text, users: [] });
  byText.get(key).users.push(c);
}

const label = (c) => `${c.name} ${c.version} (${c.license})`;
const sections = [...byText.values()]
  .map(({ text, users }) => ({ users: users.map(label).sort(), text }))
  .sort((a, b) => a.users[0].localeCompare(b.users[0]));

let out =
  "BloogBase is built with the following open source components.\n" +
  `${components.length} components; generated ${new Date().toISOString().slice(0, 10)}.\n`;
for (const s of sections) {
  out += `\n${"=".repeat(72)}\n${s.users.join("\n")}\n${"-".repeat(72)}\n\n${s.text}\n`;
}
if (withoutText.length > 0) {
  out +=
    `\n${"=".repeat(72)}\nComponents that ship no license file; their license is the one named:\n` +
    withoutText.map(label).sort().join("\n") +
    "\n";
}
writeFileSync(join(root, "public", "third-party-licenses.txt"), out);
console.log(
  `${components.length} components, ${sections.length} distinct license texts, ` +
    `${withoutText.length} without a license file`,
);
