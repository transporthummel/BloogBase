// Builds the project website (docs/website.md) into site/dist: the main page
// (page.html) and the press kit (press.html) once per language, with the texts
// from i18n/<lang>.json, the README's screenshots, the logo, fonts, styles,
// scripts and the press kit's ZIP beside them.
//
//   node site/build.mjs [--out <dir>]

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

const site = import.meta.dirname;
const root = path.resolve(site, "..");
const outArg = process.argv.indexOf("--out");
const out = path.resolve(outArg > 0 ? process.argv[outArg + 1] : path.join(site, "dist"));

const URL_BASE = "https://transporthummel.github.io/BloogBase/";
// The first language is the site's root; the others live in a folder each.
const LANGUAGES = ["en", "de"];
// Each page is built once per language, in its folder below the language's
// root.
const PAGES = [
  { template: "page.html", dir: "" },
  { template: "press.html", dir: "press/" },
];
const PRESS_ZIP = "bloogbase-press-kit.zip";
const shotsDir = path.join(root, "docs", "screenshots");
// Shown in the footer as "Last updated", in UTC: the last commit that changed
// what the pages are made of. Without git history (a shallow clone has only
// the newest commit, so the Pages workflow fetches all of it), the build time.
const SOURCES = [
  "site",
  "docs/screenshots",
  "docs/logo.png",
  "src/assets/logo.jpg",
  "src/assets/icon.png",
  "src-tauri/icons/icon.png",
  "index/mods",
];
function lastChange() {
  try {
    const date = execFileSync("git", ["log", "-1", "--format=%cI", "--", ...SOURCES], {
      cwd: root,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    if (date) return new Date(date);
  } catch {
    // No git or not a repository.
  }
  return new Date();
}
const updatedAt = lastChange();

// The number of mods in the list, for the press kit's facts.
const modCount = fs
  .readdirSync(path.join(root, "index", "mods"))
  .filter((f) => f.endsWith(".yaml")).length;

const texts = Object.fromEntries(
  LANGUAGES.map((lang) => [
    lang,
    JSON.parse(fs.readFileSync(path.join(site, "i18n", `${lang}.json`), "utf8")),
  ]),
);

// Every language must have the same texts, so no page shows a key.
const [first, ...others] = LANGUAGES;
for (const lang of others) {
  const missing = Object.keys(texts[first]).filter((k) => !(k in texts[lang]));
  const extra = Object.keys(texts[lang]).filter((k) => !(k in texts[first]));
  if (missing.length || extra.length) {
    throw new Error(
      `i18n/${lang}.json: missing ${missing.join(", ") || "none"}; extra ${extra.join(", ") || "none"}`,
    );
  }
}

/** Width and height of a PNG, from its IHDR chunk. */
function pngSize(file) {
  const buf = fs.readFileSync(file);
  return { width: buf.readUInt32BE(16), height: buf.readUInt32BE(20) };
}

/** A screenshot in this language, falling back to the English one. */
function shotFile(name, lang) {
  const localized = `${name}.${lang}.png`;
  return lang !== "en" && fs.existsSync(path.join(shotsDir, localized)) ? localized : `${name}.png`;
}

/** A file size as people read it: "398 KB", "6.9 MB". */
function fileSize(bytes, lang) {
  const format = (n, digits) =>
    new Intl.NumberFormat(lang, { maximumFractionDigits: digits }).format(n);
  return bytes < 1e6 ? `${format(bytes / 1e3, 0)} KB` : `${format(bytes / 1e6, 1)} MB`;
}

function render(page, lang) {
  const t = texts[lang];
  const template = fs.readFileSync(path.join(site, page.template), "utf8");
  // The way from the page's folder back to the site's root.
  const langDir = lang === first ? "" : `${lang}/`;
  const prefix = "../".repeat((langDir + page.dir).split("/").length - 1);
  const pageUrl = (l) => `${URL_BASE}${l === first ? "" : `${l}/`}${page.dir}`;
  const other = LANGUAGES.find((l) => l !== lang);
  const vars = {
    lang,
    root: prefix,
    // The main page in the same language.
    home: "../".repeat(page.dir.split("/").length - 1) || "./",
    base: URL_BASE,
    url: pageUrl(lang),
    otherUrl: `${prefix}${other === first ? "" : `${other}/`}${page.dir}` || "./",
    otherLang: other,
    urlEn: pageUrl("en"),
    urlDe: pageUrl("de"),
    modCount: String(modCount),
    zipSize: fileSize(fs.statSync(path.join(out, PRESS_ZIP)).size, lang),
    year: String(updatedAt.getUTCFullYear()),
    updatedIso: updatedAt.toISOString().replace(/\.\d+Z$/, "Z"),
    updated: `${new Intl.DateTimeFormat(lang, {
      dateStyle: "long",
      timeStyle: "short",
      timeZone: "UTC",
      hourCycle: "h23",
    }).format(updatedAt)} UTC`,
  };
  const html = template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (match, expr) => {
    const [kind, arg] = expr.includes(":") ? expr.split(":") : ["t", expr];
    if (kind === "t") {
      if (expr.startsWith("@")) {
        const value = vars[expr.slice(1)];
        if (value === undefined) throw new Error(`Unknown variable ${expr}`);
        return value;
      }
      if (!(expr in t)) throw new Error(`Unknown text ${expr}`);
      return t[expr];
    }
    if (kind === "shot") return `${prefix}img/${shotFile(arg, lang)}`;
    if (kind === "size") {
      const { width, height } = pngSize(path.join(shotsDir, shotFile(arg, lang)));
      return `width="${width}" height="${height}"`;
    }
    // A picture's download line, "PNG · 2560 × 1600 · 398 KB": a screenshot's
    // name or a file in img/.
    if (kind === "png") {
      const file = arg.endsWith(".png")
        ? path.join(out, "img", arg)
        : path.join(shotsDir, shotFile(arg, lang));
      const { width, height } = pngSize(file);
      return `PNG · ${width} × ${height} · ${fileSize(fs.statSync(file).size, lang)}`;
    }
    if (kind === "json") return JSON.stringify(t[arg]).replaceAll("<", "\\u003c");
    throw new Error(`Unknown placeholder ${match}`);
  });
  const dir = path.join(out, langDir, page.dir);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "index.html"), html);
}

/**
 * A ZIP archive of the given files ({ name, data }), stored without
 * compression: PNGs are compressed already.
 */
function zipFiles(files, date) {
  const time =
    (date.getUTCHours() << 11) | (date.getUTCMinutes() << 5) | (date.getUTCSeconds() >> 1);
  const day =
    ((date.getUTCFullYear() - 1980) << 9) | ((date.getUTCMonth() + 1) << 5) | date.getUTCDate();
  const parts = [];
  const central = [];
  let offset = 0;
  for (const { name, data } of files) {
    const nameBytes = Buffer.from(name, "utf8");
    const crc = zlib.crc32(data);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4); // version needed
    local.writeUInt16LE(0x0800, 6); // UTF-8 names
    local.writeUInt16LE(0, 8); // stored
    local.writeUInt16LE(time, 10);
    local.writeUInt16LE(day, 12);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(data.length, 18);
    local.writeUInt32LE(data.length, 22);
    local.writeUInt16LE(nameBytes.length, 26);
    parts.push(local, nameBytes, data);
    const entry = Buffer.alloc(46);
    entry.writeUInt32LE(0x02014b50, 0);
    entry.writeUInt16LE(20, 4); // made by
    entry.writeUInt16LE(20, 6); // version needed
    entry.writeUInt16LE(0x0800, 8);
    entry.writeUInt16LE(0, 10);
    entry.writeUInt16LE(time, 12);
    entry.writeUInt16LE(day, 14);
    entry.writeUInt32LE(crc, 16);
    entry.writeUInt32LE(data.length, 20);
    entry.writeUInt32LE(data.length, 24);
    entry.writeUInt16LE(nameBytes.length, 28);
    entry.writeUInt32LE(offset, 42);
    central.push(entry, nameBytes);
    offset += local.length + nameBytes.length + data.length;
  }
  const centralSize = central.reduce((sum, b) => sum + b.length, 0);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(files.length, 8);
  end.writeUInt16LE(files.length, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...parts, ...central, end]);
}

/** The press kit's ZIP: logo, icon, the screenshots and how to use them. */
function pressKit() {
  const plain = (html) =>
    html
      .replace(/<[^>]+>/g, "")
      .replaceAll("&nbsp;", " ")
      .replaceAll("&amp;", "&");
  const readme = LANGUAGES.map((lang) =>
    ["press.zip.title", "press.use.text", "press.use.credit", "press.zip.facts"]
      .map((key) => plain(texts[lang][key]))
      .join("\n\n"),
  ).join("\n\n---\n\n");
  const files = [
    { name: "README.txt", data: Buffer.from(`${readme}\n`.replaceAll("\n", "\r\n"), "utf8") },
    { name: "logo.png", data: fs.readFileSync(path.join(root, "docs", "logo.png")) },
    { name: "icon.png", data: fs.readFileSync(path.join(root, "src-tauri", "icons", "icon.png")) },
  ];
  // The macOS warnings are help for the README, not pictures of BloogBase.
  const shots = fs
    .readdirSync(shotsDir)
    .filter((f) => f.endsWith(".png") && !f.startsWith("macos-"))
    .sort();
  for (const file of shots) {
    const [name, ...rest] = file.split(".");
    const lang = rest.length > 1 ? rest[0] : first;
    files.push({
      name: `screenshots/${lang}/${name}.png`,
      data: fs.readFileSync(path.join(shotsDir, file)),
    });
  }
  fs.writeFileSync(path.join(out, PRESS_ZIP), zipFiles(files, updatedAt));
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "img"), { recursive: true });
for (const file of fs.readdirSync(shotsDir).filter((f) => f.endsWith(".png"))) {
  fs.copyFileSync(path.join(shotsDir, file), path.join(out, "img", file));
}
fs.copyFileSync(path.join(root, "src", "assets", "logo.jpg"), path.join(out, "img", "logo.jpg"));
fs.copyFileSync(path.join(root, "src", "assets", "icon.png"), path.join(out, "img", "icon.png"));
// Full size, for the press kit.
fs.copyFileSync(path.join(root, "docs", "logo.png"), path.join(out, "img", "logo.png"));
fs.copyFileSync(
  path.join(root, "src-tauri", "icons", "icon.png"),
  path.join(out, "img", "icon-512.png"),
);
fs.cpSync(path.join(site, "fonts"), path.join(out, "fonts"), { recursive: true });
for (const file of ["style.css", "main.js", "press.js"])
  fs.copyFileSync(path.join(site, file), path.join(out, file));
pressKit();
for (const page of PAGES) for (const lang of LANGUAGES) render(page, lang);
console.log(`wrote ${path.relative(root, out) || out}`);
