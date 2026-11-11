import type { Direction } from "../pad";

/**
 * The on-screen keyboard's keys (plans/app-controller.md §6), per language
 * and page, and how the D-pad walks them. Pure.
 */

export type KeyAction = "shift" | "symbols" | "space" | "backspace" | "done";

export type Key =
  | { type: "char"; char: string; width: number }
  | { type: "action"; action: KeyAction; width: number };

export type Page = "letters" | "symbols" | "digits";

export type Layout = Key[][];

const chars = (row: string): Key[] => [...row].map((char) => ({ type: "char", char, width: 1 }));

const LETTERS: Record<"de" | "en", string[]> = {
  de: ["1234567890ß", "qwertzuiopü", "asdfghjklöä", "yxcvbnm,.-"],
  en: ["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm,.-"],
};

/** The rest of printable ASCII, those a URL or a mod name needs first. */
const SYMBOLS = ".,-_/:?!=&@'()\"#$%+*;<>[]{}\\|^`~";

function rows(text: string, per: number) {
  const out: string[] = [];
  for (let i = 0; i < text.length; i += per) out.push(text.slice(i, i + per));
  return out;
}

const action = (a: KeyAction, width: number): Key => ({ type: "action", action: a, width });

/** Shift, symbols, Space, Backspace, Done: as wide together as a row of eleven. */
const BOTTOM: Key[] = [
  action("shift", 1.5),
  action("symbols", 1.5),
  action("space", 4),
  action("backspace", 1.5),
  action("done", 2.5),
];

export function layout(language: string, page: Page): Layout {
  if (page === "digits") {
    return [
      ...["123", "456", "789", "0"].map(chars),
      [action("backspace", 1.5), action("done", 1.5)],
    ];
  }
  const lang = language.startsWith("de") ? "de" : "en";
  const text = page === "symbols" ? rows(SYMBOLS, 11) : LETTERS[lang];
  return [...text.map(chars), BOTTOM];
}

export type Shift = "off" | "once" | "lock";

/** Shift pressed: once for a letter, again for Caps Lock, a third time off. */
export function nextShift(shift: Shift): Shift {
  return shift === "off" ? "once" : shift === "once" ? "lock" : "off";
}

/** What a key types. ß stays ß: its capital isn't on a German keyboard either. */
export function typed(char: string, shift: Shift) {
  return shift === "off" || char === "ß" ? char : char.toUpperCase();
}

export type At = { row: number; col: number };

function rowWidth(row: Key[]) {
  return row.reduce((sum, k) => sum + k.width, 0);
}

/** A key's span across, with every row centred under the widest. */
function span(keys: Layout, { row, col }: At): [number, number] {
  const widest = Math.max(...keys.map(rowWidth));
  let x = (widest - rowWidth(keys[row])) / 2;
  for (let i = 0; i < col; i++) x += keys[row][i].width;
  return [x, x + keys[row][col].width];
}

/**
 * One step on the keyboard: left and right wrap within a row, up and down
 * go to the key in the next row nearest below or above and stop at the
 * edge, as on the Xbox.
 */
export function step(keys: Layout, at: At, dir: Direction): At {
  const row = keys[at.row];
  if (dir === "left" || dir === "right") {
    const col = (at.col + (dir === "left" ? row.length - 1 : 1)) % row.length;
    return { row: at.row, col };
  }
  const next = at.row + (dir === "up" ? -1 : 1);
  if (next < 0 || next >= keys.length) return at;
  const [left, right] = span(keys, at);
  const x = (left + right) / 2;
  // The key under the middle of this one, else the nearest edge.
  const distance = (col: number) => {
    const [l, r] = span(keys, { row: next, col });
    return x < l ? l - x : x > r ? x - r : 0;
  };
  let best = 0;
  keys[next].forEach((_, col) => {
    if (distance(col) < distance(best)) best = col;
  });
  return { row: next, col: best };
}

/** The same place on another page, as near as it goes. */
export function clamp(keys: Layout, at: At): At {
  const row = Math.min(at.row, keys.length - 1);
  return { row, col: Math.min(at.col, keys[row].length - 1) };
}
