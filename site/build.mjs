// Builds the project website (docs/website.md) into site/dist: one page per
// language from page.html and i18n/<lang>.json, with the README's
// screenshots, the logo, fonts, styles and script beside it.
//
//   node site/build.mjs [--out <dir>]

import fs from "node:fs";
import path from "node:path";

const site = import.meta.dirname;
const root = path.resolve(site, "..");
const outArg = process.argv.indexOf("--out");
const out = path.resolve(outArg > 0 ? process.argv[outArg + 1] : path.join(site, "dist"));

const URL_BASE = "https://transporthummel.github.io/BloogBase/";
// The first language is the site's root; the others live in a folder each.
const LANGUAGES = ["en", "de"];
const shotsDir = path.join(root, "docs", "screenshots");
// Shown in the footer as "Last updated": when this build ran, in UTC.
const builtAt = new Date();

const template = fs.readFileSync(path.join(site, "page.html"), "utf8");
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

function render(lang) {
  const t = texts[lang];
  const prefix = lang === first ? "" : "../";
  const other = LANGUAGES.find((l) => l !== lang);
  const vars = {
    lang,
    root: prefix,
    url: lang === first ? URL_BASE : `${URL_BASE}${lang}/`,
    otherUrl: other === first ? prefix || "./" : `${prefix}${other}/`,
    otherLang: other,
    urlEn: URL_BASE,
    urlDe: `${URL_BASE}de/`,
    year: String(builtAt.getUTCFullYear()),
    updatedIso: builtAt.toISOString().replace(/\.\d+Z$/, "Z"),
    updated: `${new Intl.DateTimeFormat(lang, {
      dateStyle: "long",
      timeStyle: "short",
      timeZone: "UTC",
      hourCycle: "h23",
    }).format(builtAt)} UTC`,
  };
  const page = template.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (match, expr) => {
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
    if (kind === "json") return JSON.stringify(t[arg]).replaceAll("<", "\\u003c");
    throw new Error(`Unknown placeholder ${match}`);
  });
  const dir = lang === first ? out : path.join(out, lang);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "index.html"), page);
}

fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(path.join(out, "img"), { recursive: true });
for (const file of fs.readdirSync(shotsDir).filter((f) => f.endsWith(".png"))) {
  fs.copyFileSync(path.join(shotsDir, file), path.join(out, "img", file));
}
fs.copyFileSync(path.join(root, "src", "assets", "logo.jpg"), path.join(out, "img", "logo.jpg"));
fs.copyFileSync(path.join(root, "src", "assets", "icon.png"), path.join(out, "img", "icon.png"));
fs.cpSync(path.join(site, "fonts"), path.join(out, "fonts"), { recursive: true });
for (const file of ["style.css", "main.js"])
  fs.copyFileSync(path.join(site, file), path.join(out, file));
for (const lang of LANGUAGES) render(lang);
console.log(`wrote ${path.relative(root, out) || out}`);
