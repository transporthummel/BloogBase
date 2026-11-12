// Controller charts for docs/controller.md: one per series and the app,
// for Xbox and PlayStation controllers, in BloogBase's dark style, as SVG
// with the fonts embedded. The pad drawing follows
// src/profile/PadDrawing.tsx, the bindings src-tauri/src/keymap.rs and
// src/gamepad/; redo the charts when either changes.
//
//   node scripts/controller-charts.mjs [out dir] [en|de]
//
// English goes to docs/controller/ by default; German files get ".de".
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const OUT = process.argv[2] ?? join(ROOT, "docs/controller");
const LANG = process.argv[3] ?? "en";
const L = (de, en) => (LANG === "en" ? en : de);
mkdirSync(OUT, { recursive: true });

const b64 = (p) => readFileSync(join(ROOT, p)).toString("base64");
const INTER = b64("node_modules/@fontsource-variable/inter/files/inter-latin-wght-normal.woff2");
const PIXEL = b64(
  "node_modules/@fontsource/press-start-2p/files/press-start-2p-latin-400-normal.woff2",
);
const inner = (p) =>
  readFileSync(join(ROOT, p), "utf8")
    .replace(/^<svg[^>]*>/, "")
    .replace(/<\/svg>\s*$/, "");
const LOGO = inner("src/assets/logo.svg");
const ICON_SRC = readFileSync(join(ROOT, "src/assets/icon.svg"), "utf8");
const ICON_VB = ICON_SRC.match(/viewBox="([^"]+)"/)[1];
const ICON = inner("src/assets/icon.svg")
  .replace(/id="/g, 'id="i-')
  .replace(/url\(#/g, "url(#i-");

// Midnight Galaxy tokens (src/index.css, .local/DESIGN.md).
const C = {
  bg: "#0d0c1d",
  surface: "#181631",
  raised: "#221f42",
  border: "#34305e",
  ink: "#f2f0ff",
  muted: "#a19dc4",
  green: "#55ff55",
  yellow: "#ffff55",
  padA: "#55ff55",
  padB: "#ff5555",
  padX: "#7373ff",
  padY: "#ffff55",
  square: "#ff55ff",
  letter: "#0d0c1d",
  vorticons: "#aa00aa",
  galaxy: "#5555ff",
  dreams: "#aa5500",
};

const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

// ---- The pad drawing (900 x 350), ported from PadDrawing.tsx ----

const anchors = {
  lt: { side: "left", y: 38, line: "233,38 286,38 303,47" },
  lb: { side: "left", y: 66, line: "233,66 270,90" },
  view: { side: "left", y: 96, line: "233,96 395,96 409,124" },
  ls: { side: "left", y: 140, line: "233,140 294,140" },
  dpad: { side: "left", y: 215, line: "233,215 372,215" },
  rt: { side: "right", y: 38, line: "667,38 614,38 597,47" },
  rb: { side: "right", y: 66, line: "667,66 630,90" },
  menu: { side: "right", y: 96, line: "667,96 505,96 491,124" },
  y: { side: "right", y: 118, line: "667,118 588,113" },
  b: { side: "right", y: 142, line: "667,142 615,140" },
  a: { side: "right", y: 166, line: "667,166 588,167" },
  x: { side: "right", y: 190, line: "667,190 541,190 541,157" },
  rs: { side: "right", y: 215, line: "667,215 538,215" },
};
const psAnchors = {
  ...anchors,
  view: { side: "left", y: 96, line: "233,96 377,96" },
  dpad: { side: "left", y: 140, line: "233,140 307,140" },
  ls: { side: "left", y: 215, line: "233,215 362,215" },
  menu: { side: "right", y: 96, line: "667,96 523,96" },
};
const places = {
  xbox: { ls: [330, 140], rs: [505, 215], dpad: [395, 215] },
  playstation: { ls: [395, 215], rs: [505, 215], dpad: [330, 140] },
};
const face = [
  { input: "y", x: 570, y: 113, fill: C.padY },
  { input: "x", x: 541, y: 140, fill: C.padX },
  { input: "b", x: 599, y: 140, fill: C.padB },
  { input: "a", x: 570, y: 167, fill: C.padA },
];
const BODY =
  "M450 82 C500 80 545 73 580 78 C615 84 636 106 646 140 C659 185 672 250 676 285 " +
  "C680 320 655 338 628 334 C605 330 590 312 575 292 C560 272 540 263 500 263 L400 263 " +
  "C360 263 340 272 325 292 C310 312 295 330 272 334 C245 338 220 320 224 285 " +
  "C228 250 241 185 254 140 C264 106 285 84 320 78 C355 73 400 80 450 82 Z";
const BUMPER =
  "M266 150 L257 128 C262 98 284 70 322 62 C352 56 384 58 404 64 C412 67 416 74 418 92 Z";
const TRIGGER = "M296 70 C298 44 312 32 334 32 C356 32 370 44 372 66 L372 90 L296 90 Z";
const MIRROR = "translate(900 0) scale(-1 1)";

const NAMES = {
  xbox: { lb: "LB", rb: "RB", lt: "LT", rt: "RT", view: "View", menu: "Menu" },
  playstation: { lb: "L1", rb: "R1", lt: "L2", rt: "R2", view: "Share", menu: "Options" },
};

function psSymbol(button, r) {
  const stroke = { a: C.padX, b: C.padB, x: C.square, y: C.padA }[button];
  const w = Math.max(1.2, r / 3.5);
  const s = `fill="none" stroke="${stroke}" stroke-width="${w}"`;
  const k = r * 0.75;
  switch (button) {
    case "a":
      return `<path ${s} d="M${-k} ${-k}L${k} ${k}M${k} ${-k}L${-k} ${k}"/>`;
    case "b":
      return `<circle ${s} r="${r}"/>`;
    case "x":
      return `<rect ${s} x="${-r * 0.8}" y="${-r * 0.8}" width="${r * 1.6}" height="${r * 1.6}"/>`;
    case "y":
      return `<path ${s} stroke-linejoin="round" d="M0 ${-r * 1.15}L${r * 1.1} ${r * 0.75}L${-r * 1.1} ${r * 0.75}Z"/>`;
  }
}

function stick([x, y], op) {
  return `<g opacity="${op}">
<circle fill="${C.surface}" stroke="${C.border}" cx="${x}" cy="${y}" r="33"/>
<circle fill="${C.raised}" stroke="${C.muted}" cx="${x}" cy="${y}" r="23"/>
<circle fill="none" stroke="${C.border}" cx="${x}" cy="${y}" r="15"/></g>`;
}

function label(x, y, text, op, size = 10) {
  return `<text x="${x}" y="${y}" text-anchor="middle" dominant-baseline="central" fill="${C.muted}" opacity="${op}" font-size="${size}" font-weight="600">${esc(text)}</text>`;
}

/** callouts: { input: [first line, ...more] } */
function pad(layout, callouts) {
  const ps = layout === "playstation";
  const at = places[layout];
  const op = (i) => (callouts[i] ? 1 : 0.35);
  const cap = `fill="${C.surface}" stroke="${C.muted}"`;
  const n = NAMES[layout];
  let s = `<g stroke-width="1.5" font-family="Inter">`;
  s += `<path ${cap} opacity="${op("lt")}" d="${TRIGGER}"/>`;
  s += `<path ${cap} opacity="${op("rt")}" d="${TRIGGER}" transform="${MIRROR}"/>`;
  s += `<path ${cap} opacity="${op("lb")}" d="${BUMPER}"/>`;
  s += `<path ${cap} opacity="${op("rb")}" d="${BUMPER}" transform="${MIRROR}"/>`;
  s += `<path fill="${C.raised}" stroke="${C.border}" stroke-linejoin="round" d="${BODY}"/>`;
  s += label(334, 50, n.lt, op("lt"));
  s += label(566, 50, n.rt, op("rt"));
  s += label(312, 71, n.lb, op("lb"), 9);
  s += label(588, 71, n.rb, op("rb"), 9);
  s += stick(at.ls, op("ls"));
  s += stick(at.rs, op("rs"));
  s += `<g opacity="${op("dpad")}" transform="translate(${at.dpad[0]} ${at.dpad[1]})">
<circle fill="${C.surface}" stroke="${C.border}" r="31"/>
<path fill="${C.raised}" stroke="${C.muted}" stroke-linejoin="round" d="M-9 -23 h18 v14 h14 v18 h-14 v14 h-18 v-14 h-14 v-18 h14 Z"/></g>`;
  if (ps) {
    s += `<rect fill="${C.surface}" stroke="${C.border}" x="400" y="90" width="100" height="48" rx="8"/>`;
    s += `<circle fill="${C.surface}" stroke="${C.border}" cx="450" cy="158" r="8"/>`;
    s += `<rect ${cap} opacity="${op("view")}" x="377" y="92" width="8" height="16" rx="4"/>`;
    s += `<rect ${cap} opacity="${op("menu")}" x="515" y="92" width="8" height="16" rx="4"/>`;
  } else {
    s += `<circle fill="${C.surface}" stroke="${C.border}" cx="450" cy="104" r="12"/>`;
    s += `<g opacity="${op("view")}"><circle ${cap} cx="412" cy="134" r="9"/><rect fill="none" stroke="${C.muted}" stroke-width="1.2" x="408" y="131" width="8" height="6" rx="1"/></g>`;
    s += `<g opacity="${op("menu")}"><circle ${cap} cx="488" cy="134" r="9"/><path stroke="${C.muted}" stroke-width="1.2" d="M484 131.5 h8 M484 134 h8 M484 136.5 h8"/></g>`;
  }
  for (const f of face) {
    if (ps) {
      s += `<g opacity="${op(f.input)}" transform="translate(${f.x} ${f.y})"><circle fill="${C.letter}" stroke="${C.letter}" stroke-opacity="0.3" r="13"/>${psSymbol(f.input, 6)}</g>`;
    } else {
      s += `<g opacity="${op(f.input)}"><circle fill="${f.fill}" stroke="${C.letter}" stroke-opacity="0.3" cx="${f.x}" cy="${f.y}" r="13"/><text x="${f.x}" y="${f.y}" text-anchor="middle" dominant-baseline="central" fill="${C.letter}" font-size="12" font-weight="700">${f.input.toUpperCase()}</text></g>`;
    }
  }
  const table = ps ? psAnchors : anchors;
  for (const [input, lines] of Object.entries(callouts)) {
    const a = table[input];
    const left = a.side === "left";
    const pts = a.line.split(" ");
    const [ex, ey] = pts[pts.length - 1].split(",").map(Number);
    const x = left ? 225 : 675;
    s += `<polyline points="${a.line}" fill="none" stroke="${C.muted}" stroke-width="1" stroke-linejoin="round"/>`;
    s += `<circle fill="${C.muted}" cx="${ex}" cy="${ey}" r="2.5"/>`;
    s += `<text x="${x}" y="${a.y}" text-anchor="${left ? "end" : "start"}" dominant-baseline="middle" font-size="13">`;
    lines.forEach((line, i) => {
      s += `<tspan x="${x}" dy="${i === 0 ? 0 : 16}" fill="${i === 0 ? C.ink : C.muted}" font-size="${i === 0 ? 13 : 12}"${i === 0 ? ' font-weight="500"' : ""}>${esc(line)}</tspan>`;
    });
    s += `</text>`;
  }
  return s + `</g>`;
}

// ---- What each input does ----

function games(layout) {
  const n = NAMES[layout];
  const same = (inputs, lines) => Object.fromEntries(inputs.map((i) => [i, lines]));
  const move = {
    ls: [L("Bewegen", "Move")],
    dpad: [L("Bewegen", "Move"), L("auch schräg", "diagonals too")],
    menu: [L("Menü (Esc)", "Menu (Esc)")],
    view: [L("Halten: Menütasten", "Hold: menu keys")],
  };
  const answers = {
    a: [L("Bestätigen (Enter)", "Confirm (Enter)")],
    x: [L("Nein (N)", "No (N)")],
    y: [L("Ja (Y)", "Yes (Y)")],
  };
  const full = { rt: [L("Fenster oder Vollbild", "Window or full screen")] };
  const hold = { view: [L("Gedrückt halten", "Hold down")] };
  const letters = {
    dpad: [
      L("Buchstaben A B C D", "Letters A B C D"),
      L(`mit ${n.lb}: O P Q R`, `with ${n.lb}: O P Q R`),
    ],
    rs: [
      L("Buchstaben E F G H", "Letters E F G H"),
      L(`drücken: I · mit ${n.lb}: S T U V W`, `press: I · with ${n.lb}: S T U V W`),
    ],
    ls: [
      L("Buchstaben J K L M", "Letters J K L M"),
      L(`drücken: N · mit ${n.lb}: X Y Z`, `press: N · with ${n.lb}: X Y Z`),
    ],
  };
  const lsMore = L(
    `${n.lb} + links: Leertaste, ${n.lb} + drücken: Rücktaste`,
    `${n.lb} + left: space, ${n.lb} + press: backspace`,
  );
  const notes = [
    L(
      "Schließ den Controller an, bevor du das Spiel startest. Später findet DOSBox ihn nicht mehr.",
      "Connect the controller before you start the game. DOSBox won't find it later.",
    ),
    L(
      "Die Knöpfe drücken die Tasten, die im Spiel eingestellt sind; geänderte Tasten folgen beim nächsten Start.",
      "The buttons press the keys the game is set to; changed keys follow at the next start.",
    ),
    L(
      "Schräg geht es nur mit dem Steuerkreuz, nicht mit dem Stick.",
      "Diagonals only work on the D-pad, not on the stick.",
    ),
  ];
  const order = L("Richtungen: hoch, rechts, runter, links", "Directions: up, right, down, left");
  return {
    vorticons: {
      title: "Keen 1-3",
      sub: L("Commander Keen 1–3 und ihre Mods", "Commander Keen 1–3 and their mods"),
      colour: C.vorticons,
      tile: "1-3",
      panels: [
        {
          heading: L("Spielen", "Playing"),
          callouts: {
            a: [L("Springen", "Jump")],
            ...same(["b", "lb", "rb"], [L("Pogo an und aus", "Pogo on and off")]),
            ...same(["x", "lt", "rt"], [L("Schießen", "Shoot")]),
            y: [L("Statusfenster", "Status window")],
            ...move,
          },
        },
        {
          heading: L(`Menüs (${n.view} halten)`, `Menus (hold ${n.view})`),
          aside: order,
          callouts: {
            ...answers,
            b: [L("Zum Titelbild (T)", "To the title screen (T)")],
            menu: [L("Zu DOS beenden (D)", "Quit to DOS (D)")],
            rb: [L("Speichern (F5)", "Save (F5)")],
            lb: [L("Sound an und aus (F2)", "Sound on and off (F2)")],
            ...full,
            rs: [
              L("Spielstand 1 2 3 4", "Save slot 1 2 3 4"),
              L("drücken: Spielstand 9", "press: save slot 9"),
            ],
            dpad: [L("Spielstand 5 6 7 8", "Save slot 5 6 7 8")],
            ...hold,
          },
        },
      ],
      notes: [
        ...notes,
        L(
          `Ohne „Schießen mit einer Taste“ (Einstellungen) schießen ${layout === "playstation" ? "Quadrat, L2 und R2" : "X, LT und RT"} mit Springen und Pogo zusammen.`,
          `Without one-button firing (Settings), ${layout === "playstation" ? "Square, L2 and R2" : "X, LT and RT"} shoot with Jump and Pogo together.`,
        ),
      ],
    },
    galaxy: {
      title: "Keen 4-6",
      sub: L("Commander Keen 4–6 und ihre Mods", "Commander Keen 4–6 and their mods"),
      colour: C.galaxy,
      tile: "4-6",
      panels: [
        {
          heading: L("Spielen", "Playing"),
          callouts: {
            a: [L("Springen", "Jump")],
            ...same(["b", "lb", "rb"], ["Pogo"]),
            ...same(["x", "lt", "rt"], [L("Schießen", "Shoot")]),
            y: [L("Statusfenster", "Status window")],
            ...move,
          },
        },
        {
          heading: L(`Menüs (${n.view} halten)`, `Menus (hold ${n.view})`),
          aside: order,
          callouts: {
            ...answers,
            b: [L("Zurück (Esc)", "Back (Esc)")],
            lb: [L("Halten: weitere Buchstaben", "Hold: more letters")],
            ...full,
            ...letters,
            ...hold,
          },
        },
      ],
      notes: [
        ...notes,
        L(`Beim Namen für einen Spielstand: ${lsMore}.`, `Naming a saved game: ${lsMore}.`),
      ],
    },
    dreams: {
      title: "Keen Dreams",
      sub: L(
        "Commander Keen in Keen Dreams und seine Mods",
        "Commander Keen in Keen Dreams and its mods",
      ),
      colour: C.dreams,
      tile: "KD",
      panels: [
        {
          heading: L("Spielen", "Playing"),
          callouts: {
            a: [L("Springen", "Jump")],
            ...same(["x", "lt", "rt"], [L("Flower Power werfen", "Throw Flower Power")]),
            y: [L("Statusfenster", "Status window")],
            ...move,
          },
        },
        {
          heading: L(`Menüs (${n.view} halten)`, `Menus (hold ${n.view})`),
          aside: order,
          callouts: {
            ...answers,
            b: [L("Zurück (Esc)", "Back (Esc)")],
            lb: [L("Halten: weitere Buchstaben", "Hold: more letters")],
            ...full,
            ...letters,
            ...hold,
          },
        },
      ],
      notes: [
        ...notes,
        L(`Beim Namen für einen Spielstand: ${lsMore}.`, `Naming a saved game: ${lsMore}.`),
      ],
    },
    app: {
      title: L("Die App", "The App"),
      sub: L(
        "BloogBase selbst mit dem Controller bedienen",
        "Using BloogBase itself with the controller",
      ),
      colour: null,
      panels: [
        {
          heading: L("Bedienen", "Getting around"),
          callouts: {
            ls: [L("Fokus bewegen", "Move the focus")],
            dpad: [L("Fokus bewegen", "Move the focus")],
            a: [
              L("Auswählen", "Select"),
              L("Textfeld: Tastatur öffnen", "text field: open the keyboard"),
            ],
            b: [L("Zurück oder schließen", "Back or close")],
            rs: [L("Scrollen", "Scroll")],
            lb: [L("Vorheriger Tab", "Previous tab")],
            rb: [L("Nächster Tab", "Next tab")],
            lt: [L("Seite hoch", "Page up")],
            rt: [L("Seite runter", "Page down")],
            view: [L("Zur Seitenleiste", "To the sidebar"), L("noch mal: zurück", "again: back")],
            menu: [L("Zur Seitenleiste", "To the sidebar"), L("noch mal: zurück", "again: back")],
          },
        },
        {
          heading: L("Bildschirmtastatur", "On-screen keyboard"),
          aside: L(
            `öffnet sich mit ${layout === "playstation" ? "Kreuz" : "A"} auf einem Textfeld`,
            `opens with ${layout === "playstation" ? "Cross" : "A"} on a text field`,
          ),
          callouts: {
            dpad: [L("Taste wählen", "Pick a key")],
            ls: [L("Taste wählen", "Pick a key"), L("drücken: Umschalt", "press: Shift")],
            a: [L("Tippen", "Type")],
            x: [L("Löschen", "Backspace")],
            y: [L("Leertaste", "Space")],
            lb: [L("Cursor links", "Cursor left")],
            rb: [L("Cursor rechts", "Cursor right")],
            lt: [L("Symbole", "Symbols")],
            menu: [L("Fertig", "Done")],
            b: [L("Schließen", "Close")],
          },
        },
      ],
      notes: [
        L(
          "BloogBase sieht den Controller nach dem ersten Knopfdruck im Fenster.",
          "BloogBase sees the controller after the first button press in its window.",
        ),
        L(
          "Läuft ein Spiel, gehört der Controller dem Spiel; BloogBase hört dann nicht hin.",
          "While a game runs, the controller belongs to the game; BloogBase doesn't listen.",
        ),
        L(
          "Tastatur und Maus funktionieren daneben weiter.",
          "Keyboard and mouse keep working alongside.",
        ),
      ],
    },
  };
}

// ---- The poster ----

const W = 1200;
const M = 48;
const PANEL_H = 60 + 350 + 8;

function stars(h) {
  // Fixed seed, so every chart has the same sky.
  let seed = 7;
  const rnd = () => (seed = (seed * 16807) % 2147483647) / 2147483647;
  let s = "";
  for (let i = 0; i < 70; i++) {
    const x = Math.round(rnd() * W);
    const y = Math.round(rnd() * h);
    const r = rnd();
    const size = r > 0.92 ? 3 : 2;
    const fill = r > 0.96 ? C.yellow : r > 0.5 ? C.ink : C.muted;
    s += `<rect x="${x}" y="${y}" width="${size}" height="${size}" fill="${fill}" opacity="${(0.15 + rnd() * 0.35).toFixed(2)}"/>`;
  }
  return s;
}

function poster(sheet, layout) {
  const notesH = 44 + sheet.notes.length * 24 + 20;
  const top = 212;
  const H = top + sheet.panels.length * (PANEL_H + 24) + notesH + M;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${W} ${H}" width="${W}" height="${H}">
<style>
@font-face{font-family:"Inter";font-weight:100 900;src:url(data:font/woff2;base64,${INTER}) format("woff2")}
@font-face{font-family:"Press Start 2P";src:url(data:font/woff2;base64,${PIXEL}) format("woff2")}

</style>
<g font-family="Inter, system-ui, sans-serif"><rect width="${W}" height="${H}" fill="${C.bg}"/>
${stars(H)}
<svg x="${M - 4}" y="32" width="${Math.round((677 * 48) / 175)}" height="48" viewBox="0 0 677 175">${LOGO}</svg>`;
  // Title row: the engine's tile (or the app icon), title and subtitle.
  if (sheet.colour) {
    s += `<rect x="${M}" y="112" width="64" height="64" rx="8" fill="${sheet.colour}"/>`;
    s += `<text x="${M + 32}" y="145" text-anchor="middle" dominant-baseline="central" font-family="'Press Start 2P', monospace" font-size="14" fill="#ffffff">${sheet.tile}</text>`;
  } else {
    s += `<svg x="${M}" y="112" width="64" height="64" viewBox="${ICON_VB}">${ICON}</svg>`;
  }
  const kind =
    (layout === "playstation" ? "PlayStation" : "Xbox") + L("-Controller", " controller");
  s += `<text x="${M + 84}" y="134" font-family="'Press Start 2P', monospace" font-size="22" fill="${C.ink}">${esc(sheet.title)}</text>`;
  s += `<text x="${M + 84}" y="166" font-size="16" fill="${C.muted}">${esc(sheet.sub)}</text>`;
  // The layout as a badge at the right.
  const badge = kind.toUpperCase();
  const bw = badge.length * 10 + 24;
  s += `<rect x="${W - M - bw}" y="128" width="${bw}" height="30" rx="4" fill="${C.raised}" stroke="${C.border}"/>`;
  s += `<text x="${W - M - bw / 2}" y="144" text-anchor="middle" dominant-baseline="central" font-family="'Press Start 2P', monospace" font-size="10" fill="${C.muted}">${badge}</text>`;

  let y = top;
  for (const p of sheet.panels) {
    s += `<rect x="${M}" y="${y}" width="${W - 2 * M}" height="${PANEL_H}" rx="12" fill="${C.surface}" stroke="${C.border}"/>`;
    s += `<text x="${M + 24}" y="${y + 38}" font-size="18" font-weight="600" fill="${C.ink}">${esc(p.heading)}</text>`;
    if (p.aside) {
      s += `<text x="${W - M - 24}" y="${y + 38}" text-anchor="end" font-size="13" fill="${C.muted}">${esc(p.aside)}</text>`;
    }
    s += `<g transform="translate(${(W - 900) / 2} ${y + 52})">${pad(layout, p.callouts)}</g>`;
    y += PANEL_H + 24;
  }
  s += `<text x="${M}" y="${y + 24}" font-size="15" font-weight="600" fill="${C.ink}">${L("Gut zu wissen", "Good to know")}</text>`;
  sheet.notes.forEach((note, i) => {
    const ny = y + 54 + i * 24;
    s += `<rect x="${M + 2}" y="${ny - 6}" width="5" height="5" fill="${C.green}"/>`;
    s += `<text x="${M + 18}" y="${ny}" dominant-baseline="middle" font-size="14" fill="${C.muted}">${esc(note)}</text>`;
  });
  return { svg: s + `</g></svg>`, H };
}

const sizes = {};
for (const layout of ["xbox", "playstation"]) {
  const all = games(layout);
  for (const [key, sheet] of Object.entries(all)) {
    const name = `${key === "vorticons" ? "keen1-3" : key === "galaxy" ? "keen4-6" : key === "dreams" ? "keen-dreams" : "app"}-${layout}`;
    const { svg, H } = poster(sheet, layout);
    writeFileSync(join(OUT, `${name}${LANG === "en" ? "" : ".de"}.svg`), svg);
    sizes[name + (LANG === "en" ? "" : ".de")] = H;
  }
}
console.log(`${Object.keys(sizes).length} charts in ${OUT}`);
