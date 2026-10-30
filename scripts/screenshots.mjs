// Takes the README's screenshots (docs/screenshots.md): starts a second dev
// instance with its own data folder, fills it with games and mods once, then
// drives the app over the Chrome DevTools Protocol and writes each shot in
// docs/screenshots/shots.json to docs/screenshots/<name>.png.
//
//   node scripts/screenshots.mjs [--fresh] [--only name,name] [--attach]
//
// --fresh   start from an empty data folder (games and mods again)
// --only    take only these shots
// --keep    leave the dev instance running afterwards, for --attach
// --attach  use the instance a --keep run left running; no seeding
//
// Windows only: CDP comes from WebView2.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";

const root = path.resolve(import.meta.dirname, "..");
const shotsDir = path.join(root, "docs", "screenshots");
const manifest = JSON.parse(fs.readFileSync(path.join(shotsDir, "shots.json"), "utf8"));
const work = path.resolve(root, process.env.SCREENSHOTS_DIR ?? ".local/tmp/screenshots");
const dataDir = path.join(work, "data");
const gamesDir = path.resolve(root, process.env.SCREENSHOTS_GAMES ?? ".local/games");
const PORT = 1430;
const CDP_PORT = 9333;

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const onlyArg = args[args.indexOf("--only") + 1];
const only = flag("--only") && onlyArg ? onlyArg.split(",") : null;
const attach = flag("--attach");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

if (only) {
  const unknown = only.filter((name) => !manifest.shots.some((s) => s.name === name));
  if (unknown.length) throw new Error(`Unknown shots: ${unknown.join(", ")}`);
}

// --- Dev instance -----------------------------------------------------------

let app = null;

function startApp() {
  const config = JSON.stringify({
    build: { devUrl: `http://localhost:${PORT}`, beforeDevCommand: `pnpm dev --port ${PORT}` },
  });
  const env = {
    ...process.env,
    BLOOGBASE_DATA_DIR: dataDir,
    // Absolute: a relative one resolves from src-tauri/.
    CARGO_TARGET_DIR: path.join(root, ".local", "target-cdp"),
    WEBVIEW2_USER_DATA_FOLDER: path.join(work, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
  };
  const log = fs.openSync(path.join(work, "dev.log"), "w");
  // A file, so the JSON's quotes needn't survive cmd.exe.
  const configFile = path.join(work, "tauri.json");
  fs.writeFileSync(configFile, config);
  app = spawn(`pnpm tauri dev --config "${configFile}"`, {
    cwd: root,
    env,
    shell: true,
    stdio: ["ignore", log, log],
  });
  if (flag("--keep")) app.unref();
  console.log(`Starting the app (log: ${path.relative(root, path.join(work, "dev.log"))})…`);
}

function stopApp() {
  if (!app) return;
  spawnSync("taskkill", ["/pid", String(app.pid), "/t", "/f"], { stdio: "ignore" });
  stopLeftovers();
  app = null;
}

/** The app and Vite of an earlier run sometimes outlive their parent. */
function stopLeftovers() {
  const script = [
    "Get-Process bloogbase -ErrorAction SilentlyContinue | Where-Object { $_.Path -like '*target-cdp*' } | Stop-Process -Force",
    `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*--port ${PORT}*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`,
  ].join("; ");
  spawnSync("powershell", ["-NoProfile", "-Command", script], { stdio: "ignore" });
}

process.on("exit", () => flag("--keep") || stopApp());
process.on("SIGINT", () => process.exit(130));

// --- CDP ----------------------------------------------------------------------

let ws;
let nextId = 0;
const pending = new Map();

async function connect(timeoutMs) {
  const until = Date.now() + timeoutMs;
  for (;;) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${CDP_PORT}/json`)).json();
      const page = targets.find((t) => t.type === "page" && t.url.includes(`localhost:${PORT}`));
      if (page) {
        ws = new WebSocket(page.webSocketDebuggerUrl);
        ws.onmessage = (e) => {
          const msg = JSON.parse(e.data);
          if (msg.id && pending.has(msg.id)) {
            pending.get(msg.id)(msg);
            pending.delete(msg.id);
          }
        };
        // The webview may replace its page while the app starts.
        ws.onclose = () => {
          ws = null;
          for (const answer of pending.values())
            answer({ error: { message: "connection closed" } });
          pending.clear();
        };
        await new Promise((resolve, reject) => {
          ws.onopen = resolve;
          ws.onerror = reject;
        });
        return;
      }
    } catch {
      // Not up yet.
    }
    if (app && app.exitCode !== null) throw new Error("The dev instance stopped; see dev.log.");
    if (Date.now() > until) throw new Error("The app didn't come up in time; see dev.log.");
    await sleep(1000);
  }
}

async function send(method, params = {}) {
  if (!ws) await connect(60000);
  return new Promise((resolve, reject) => {
    const id = ++nextId;
    // An evaluation can hang while the page reloads.
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method}: no answer`));
    }, 30000);
    pending.set(id, (msg) => {
      clearTimeout(timer);
      if (msg.error) reject(new Error(`${method}: ${msg.error.message}`));
      else resolve(msg.result);
    });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (result.exceptionDetails) {
    throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
  }
  return result.result.value;
}

const invoke = (cmd, payload = {}) =>
  evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(payload)})`);

async function waitUntil(expression, what, timeoutMs = 20000) {
  const until = Date.now() + timeoutMs;
  while (!(await evaluate(expression).catch(() => false))) {
    if (Date.now() > until) throw new Error(`Timed out waiting for ${what}`);
    await sleep(200);
  }
}

// A first load sometimes stalls while Vite optimizes dependencies; a reload
// gets it going.
async function appReady() {
  const ready =
    "document.readyState === 'complete' && !!window.__TAURI_INTERNALS__ && !!document.querySelector('#root > *')";
  for (let attempt = 1; ; attempt++) {
    try {
      return await waitUntil(ready, "the app", 30000);
    } catch (e) {
      if (attempt === 3) throw e;
      await send("Page.reload");
    }
  }
}

// --- Seeding ------------------------------------------------------------------

/** Waits for all jobs; returns the failed ones. */
async function waitForJobs() {
  for (;;) {
    const jobs = await invoke("list_jobs");
    if (jobs.every((j) => j.phase === "done" || j.phase === "failed")) {
      await invoke("clear_finished_jobs");
      return jobs.filter((j) => j.phase === "failed");
    }
    await sleep(1000);
  }
}

const warnFailed = (jobs) => {
  for (const job of jobs)
    console.warn(`  failed: ${job.kind} ${job.subject ?? ""}: ${job.error?.message}`);
};

async function seed() {
  const s = manifest.seed;
  console.log("Seeding the data folder…");
  await invoke("complete_setup");
  for (const folder of s.games) {
    const installs = await invoke("import_games", { path: path.join(gamesDir, folder) });
    console.log(
      `  ${folder}: ${installs.map((i) => i.label ?? i.gameId).join(", ") || "nothing new"}`,
    );
  }
  for (const indexId of s.mods) {
    await invoke("install_index_mod", {
      indexId,
      installId: null,
      withoutGame: false,
      archive: null,
    });
  }
  warnFailed(await waitForJobs());
  // Alone, after the mods: next to other downloads, unpacking it sometimes
  // ended in "access denied".
  await invoke("install_runtime");
  warnFailed(await waitForJobs());

  // Play time can't come from playing here, so it goes straight into the
  // database, ending at the given time ago.
  const profiles = await listProfiles();
  const db = new DatabaseSync(path.join(dataDir, "bloogbase.db"));
  db.exec("PRAGMA busy_timeout = 5000");
  const insert = db.prepare(
    "INSERT INTO play_sessions (profile_id, started_at, ended_at, exit_code) VALUES (?, ?, ?, 0)",
  );
  const now = Math.floor(Date.now() / 1000);
  for (const session of s.sessions ?? []) {
    const profile = findProfile(profiles, session.profile);
    const end = now - Math.round((session.hoursAgo ?? 0) * 3600);
    insert.run(profile.id, end - session.minutes * 60, end);
  }
  db.close();

  // Saves from other copies, as if played here.
  for (const saves of s.saves ?? []) {
    const state = path.join(dataDir, "profiles", findProfile(profiles, saves.profile).id, "state");
    fs.mkdirSync(state, { recursive: true });
    for (const file of saves.files)
      fs.copyFileSync(path.join(gamesDir, saves.from, file), path.join(state, file));
  }

  for (const backup of s.backups ?? []) {
    const profile = findProfile(profiles, backup.profile);
    await invoke("create_backup", {
      profileId: profile.id,
      label: backup.label ?? null,
      file: null,
    });
  }
  fs.writeFileSync(path.join(work, "seeded"), new Date().toISOString());
}

/** Profiles, each with the mod list id of its mod, for `findProfile`. */
async function listProfiles() {
  const [profiles, mods] = await Promise.all([invoke("list_profiles"), invoke("list_mods")]);
  return profiles.map((p) => ({ ...p, indexId: mods.find((m) => m.id === p.modId)?.indexId }));
}

/** The profile with this id, mod id, mod list id or label, or a game's own. */
function findProfile(profiles, ref) {
  const found =
    profiles.find((p) => [p.id, p.modId, p.indexId, p.label].includes(ref)) ??
    profiles.find((p) => p.modId === null && p.gameId === ref);
  if (!found) throw new Error(`No profile "${ref}"; have: ${profiles.map((p) => p.id).join(", ")}`);
  return found;
}

// --- Shots --------------------------------------------------------------------

const textMatch = (
  text,
  selector = "button, a, [role=tab], [role=option], [role=menuitem], label",
) =>
  `[...document.querySelectorAll(${JSON.stringify(selector)})].find((el) => el.offsetParent !== null && el.textContent.trim().startsWith(${JSON.stringify(text)}))`;

async function runStep(raw, language) {
  // Texts can differ per language: { "en": "Saves", "de": "Spielstände" }.
  const step = Object.fromEntries(
    Object.entries(raw).map(([k, v]) => [
      k,
      v && typeof v === "object" && !Array.isArray(v) ? v[language] : v,
    ]),
  );
  if (step.click) {
    await waitUntil(`!!document.querySelector(${JSON.stringify(step.click)})`, step.click);
    await evaluate(`document.querySelector(${JSON.stringify(step.click)}).click()`);
  } else if (step.clickText) {
    await waitUntil(`!!(${textMatch(step.clickText, step.selector)})`, `"${step.clickText}"`);
    await evaluate(`(${textMatch(step.clickText, step.selector)}).click()`);
  } else if (step.focus) {
    await waitUntil(`!!document.querySelector(${JSON.stringify(step.focus)})`, step.focus);
    await evaluate(`document.querySelector(${JSON.stringify(step.focus)}).focus()`);
  } else if (step.fill) {
    // The value setter React listens to, as typing would.
    await waitUntil(`!!document.querySelector(${JSON.stringify(step.fill)})`, step.fill);
    await evaluate(`(() => {
      const input = document.querySelector(${JSON.stringify(step.fill)});
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set.call(input, ${JSON.stringify(step.value)});
      input.dispatchEvent(new Event("input", { bubbles: true }));
      input.focus();
    })()`);
  } else if (step.type) {
    await send("Input.insertText", { text: step.type });
  } else if (step.key) {
    const key = {
      key: step.key,
      code: step.key,
      windowsVirtualKeyCode: { Enter: 13, Escape: 27, Tab: 9 }[step.key],
    };
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...key });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...key });
  } else if (step.waitFor) {
    await waitUntil(
      `!!document.querySelector(${JSON.stringify(step.waitFor)})`,
      step.waitFor,
      step.timeout,
    );
  } else if (step.waitText) {
    await waitUntil(
      `document.body.innerText.includes(${JSON.stringify(step.waitText)})`,
      `"${step.waitText}"`,
      step.timeout,
    );
  } else if (step.wait) {
    await sleep(step.wait);
  } else if (step.eval) {
    await evaluate(step.eval);
  } else {
    throw new Error(`Unknown step ${JSON.stringify(step)}`);
  }
}

// Images loaded, nothing marked busy, then a moment for the last paint.
const settle = () =>
  waitUntil(
    "[...document.images].every((img) => img.complete) && !document.querySelector('[aria-busy=true]')",
    "the page to settle",
  )
    .catch(() => console.warn("  (page didn't settle; taking the shot anyway)"))
    .then(() => sleep(manifest.settle ?? 800));

/** The file a shot writes in this language: `<name>.png` in English, `<name>.<lang>.png` otherwise. */
const fileName = (shot, language) =>
  language === "en" ? `${shot.name}.png` : `${shot.name}.${language}.png`;

async function take(shot, language) {
  const width = shot.width ?? manifest.window.width;
  const height = shot.height ?? manifest.window.height;
  const scale = shot.scale ?? manifest.window.scale ?? 1;
  await send("Emulation.setDeviceMetricsOverride", {
    width,
    height,
    deviceScaleFactor: scale,
    mobile: false,
  });
  await send("Emulation.setEmulatedMedia", {
    features: [{ name: "prefers-reduced-motion", value: "reduce" }],
  });

  // Preferences live in localStorage; a fresh load at the route starts every
  // shot from the same state.
  const prefs = {
    "bloogbase.theme": shot.theme ?? "dark",
    "bloogbase.language": language,
    ...manifest.localStorage,
    ...shot.localStorage,
  };
  await evaluate(
    `Object.entries(${JSON.stringify(prefs)}).forEach(([k, v]) => localStorage.setItem(k, v))`,
  );

  for (const call of shot.invoke ?? []) await invoke(call.cmd, call.args);

  let route = shot.route ?? "/";
  if (route.includes("{")) {
    const profiles = await listProfiles();
    route = route.replace(/\{profile:([^}]+)\}/g, (_, ref) => findProfile(profiles, ref).id);
  }
  await send("Page.navigate", { url: `http://localhost:${PORT}${route}` });
  await sleep(300);
  await appReady();
  await settle();
  for (const step of shot.steps ?? []) await runStep(step, language);
  await settle();

  const params = { format: "png" };
  if (shot.clip) {
    const rect = await evaluate(
      `(() => { const r = document.querySelector(${JSON.stringify(shot.clip)}).getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; })()`,
    );
    const pad = shot.clipPadding ?? 0;
    params.clip = {
      x: rect.x - pad,
      y: rect.y - pad,
      width: rect.width + 2 * pad,
      height: rect.height + 2 * pad,
      scale: 1,
    };
  }
  const { data } = await send("Page.captureScreenshot", params);
  fs.writeFileSync(path.join(shotsDir, fileName(shot, language)), Buffer.from(data, "base64"));
  console.log(`  ${fileName(shot, language)}`);
}

// --- Main ---------------------------------------------------------------------

try {
  let needsSeed = false;
  if (!attach) {
    stopLeftovers();
    if (flag("--fresh")) fs.rmSync(work, { recursive: true, force: true });
    needsSeed = !fs.existsSync(path.join(work, "seeded"));
    if (needsSeed) fs.rmSync(dataDir, { recursive: true, force: true });
    fs.mkdirSync(dataDir, { recursive: true });
    startApp();
  }
  // The first build of the dev instance takes a while.
  await connect(20 * 60 * 1000);
  await appReady();
  if (needsSeed) await seed();

  console.log("Taking screenshots…");
  for (const shot of manifest.shots) {
    if (only && !only.includes(shot.name)) continue;
    for (const language of shot.languages ?? ["en"]) await take(shot, language);
  }
  await send("Emulation.clearDeviceMetricsOverride");
} finally {
  ws?.close();
  if (!flag("--keep")) stopApp();
}
