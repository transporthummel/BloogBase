// Runs every check CI runs, plus all ignored Rust tests (downloads, DOSBox
// windows, mod corpus, cover extraction). Meant for before pushing bigger
// changes: `pnpm test:all`. Keeps going after a failure and lists all
// failed steps at the end.
//
// The ignored tests read their inputs from .local; override with
// BLOOGBASE_E2E_GAME, BLOOGBASE_E2E_DOSBOX, BLOOGBASE_MOD_CACHE, KEEN_GAMES.
import { execFileSync, execSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const local = join(root, ".local");
const app = join(root, "src-tauri");
const tool = join(root, "index", "tool");
const windows = process.platform === "win32";

const env = {
  ...process.env,
  BLOOGBASE_E2E_GAME: process.env.BLOOGBASE_E2E_GAME ?? join(local, "games", "keen4"),
  BLOOGBASE_E2E_DOSBOX:
    process.env.BLOOGBASE_E2E_DOSBOX ??
    join(local, "tmp", "dosbox", "dosbox-staging-v0.83.0", "dosbox.exe"),
  BLOOGBASE_MOD_CACHE: process.env.BLOOGBASE_MOD_CACHE ?? join(local, "mods"),
  KEEN_GAMES: process.env.KEEN_GAMES ?? join(local, "games"),
  KEEN_MODS: join(app, "target", "covers-mods"),
  KEEN_OUT: join(app, "target", "covers-out"),
};

const failed = [];

/** Runs one step, printing its output live; records it if it fails. */
function step(name, cmd, args, cwd = root) {
  console.log(`\n=== ${name} ===`);
  try {
    // pnpm is a .cmd script on Windows, which only runs in a shell; the
    // arguments are fixed, so that's safe.
    if (cmd === "pnpm" && windows) {
      execSync(`pnpm ${args.join(" ")}`, { cwd, env, stdio: "inherit" });
    } else {
      execFileSync(cmd, args, { cwd, env, stdio: "inherit" });
    }
  } catch {
    failed.push(name);
  }
}

/** Unpacks every mod zip from the mod cache into KEEN_MODS, one folder each. */
function unpackMods() {
  rmSync(env.KEEN_MODS, { recursive: true, force: true });
  for (const zip of readdirSync(env.BLOOGBASE_MOD_CACHE)) {
    if (!zip.endsWith(".zip")) continue;
    const out = join(env.KEEN_MODS, zip.slice(0, -4));
    mkdirSync(out, { recursive: true });
    const archive = join(env.BLOOGBASE_MOD_CACHE, zip);
    // Windows' own tar (bsdtar) reads zips; a GNU tar earlier on PATH doesn't.
    if (windows) {
      execFileSync(join(process.env.SystemRoot, "System32", "tar.exe"), [
        "-xf",
        archive,
        "-C",
        out,
      ]);
    } else {
      execFileSync("unzip", ["-q", archive, "-d", out]);
    }
  }
  mkdirSync(env.KEEN_OUT, { recursive: true });
}

for (const name of ["BLOOGBASE_E2E_GAME", "BLOOGBASE_E2E_DOSBOX", "KEEN_GAMES"]) {
  if (!existsSync(env[name])) {
    console.error(`${name} not found: ${env[name]}`);
    process.exit(1);
  }
}

step("Lint", "pnpm", ["lint"]);
step("Format", "pnpm", ["format:check"]);
step("Typecheck", "pnpm", ["typecheck"]);
step("Frontend tests", "pnpm", ["test"]);
step("Frontend build", "pnpm", ["build"]);

for (const [label, cwd] of [
  ["App", app],
  ["Index tool", tool],
]) {
  step(`${label}: fmt`, "cargo", ["fmt", "--check"], cwd);
  step(`${label}: clippy`, "cargo", ["clippy", "--all-targets", "--", "-D", "warnings"], cwd);
  step(`${label}: tests`, "cargo", ["test"], cwd);
}

// cargo test regenerates src/bindings.ts.
step("Bindings up to date", "git", ["diff", "--exit-code", "src/bindings.ts"]);

console.log("\n=== Unpacking mods for the cover test ===");
try {
  unpackMods();
} catch (e) {
  console.error(e.message);
  failed.push("Unpacking mods");
}
// One at a time: the DOSBox tests open windows and share the screen.
step("App: ignored tests", "cargo", ["test", "--", "--ignored", "--test-threads=1"], app);
step("Index entries and downloads", "pnpm", ["index:check"]);

if (failed.length) {
  console.error(`\nFailed:\n${failed.map((f) => `  - ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("\nAll checks passed.");
