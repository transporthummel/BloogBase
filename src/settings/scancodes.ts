import type { GameKeys } from "../bindings";

/**
 * Which DOS scancode a key sends. Letters go by what's printed on the key, as
 * the mapper file for non-US layouts makes that key send it (`keymap.rs`);
 * everything else by the key's position.
 */
const LETTERS: Record<string, number> = {
  a: 0x1e,
  b: 0x30,
  c: 0x2e,
  d: 0x20,
  e: 0x12,
  f: 0x21,
  g: 0x22,
  h: 0x23,
  i: 0x17,
  j: 0x24,
  k: 0x25,
  l: 0x26,
  m: 0x32,
  n: 0x31,
  o: 0x18,
  p: 0x19,
  q: 0x10,
  r: 0x13,
  s: 0x1f,
  t: 0x14,
  u: 0x16,
  v: 0x2f,
  w: 0x11,
  x: 0x2d,
  y: 0x15,
  z: 0x2c,
};

const CODES: Record<string, number> = {
  Escape: 0x01,
  Digit1: 0x02,
  Digit2: 0x03,
  Digit3: 0x04,
  Digit4: 0x05,
  Digit5: 0x06,
  Digit6: 0x07,
  Digit7: 0x08,
  Digit8: 0x09,
  Digit9: 0x0a,
  Digit0: 0x0b,
  Minus: 0x0c,
  Equal: 0x0d,
  Backspace: 0x0e,
  Tab: 0x0f,
  KeyQ: 0x10,
  KeyW: 0x11,
  KeyE: 0x12,
  KeyR: 0x13,
  KeyT: 0x14,
  KeyY: 0x15,
  KeyU: 0x16,
  KeyI: 0x17,
  KeyO: 0x18,
  KeyP: 0x19,
  BracketLeft: 0x1a,
  BracketRight: 0x1b,
  Enter: 0x1c,
  NumpadEnter: 0x1c,
  ControlLeft: 0x1d,
  ControlRight: 0x1d,
  KeyA: 0x1e,
  KeyS: 0x1f,
  KeyD: 0x20,
  KeyF: 0x21,
  KeyG: 0x22,
  KeyH: 0x23,
  KeyJ: 0x24,
  KeyK: 0x25,
  KeyL: 0x26,
  Semicolon: 0x27,
  Quote: 0x28,
  Backquote: 0x29,
  ShiftLeft: 0x2a,
  Backslash: 0x2b,
  KeyZ: 0x2c,
  KeyX: 0x2d,
  KeyC: 0x2e,
  KeyV: 0x2f,
  KeyB: 0x30,
  KeyN: 0x31,
  KeyM: 0x32,
  Comma: 0x33,
  Period: 0x34,
  Slash: 0x35,
  NumpadDivide: 0x35,
  ShiftRight: 0x36,
  NumpadMultiply: 0x37,
  AltLeft: 0x38,
  AltRight: 0x38,
  Space: 0x39,
  CapsLock: 0x3a,
  F1: 0x3b,
  F2: 0x3c,
  F3: 0x3d,
  F4: 0x3e,
  F5: 0x3f,
  F6: 0x40,
  F7: 0x41,
  F8: 0x42,
  F9: 0x43,
  F10: 0x44,
  NumLock: 0x45,
  ScrollLock: 0x46,
  Home: 0x47,
  Numpad7: 0x47,
  ArrowUp: 0x48,
  Numpad8: 0x48,
  PageUp: 0x49,
  Numpad9: 0x49,
  NumpadSubtract: 0x4a,
  ArrowLeft: 0x4b,
  Numpad4: 0x4b,
  Numpad5: 0x4c,
  ArrowRight: 0x4d,
  Numpad6: 0x4d,
  NumpadAdd: 0x4e,
  End: 0x4f,
  Numpad1: 0x4f,
  ArrowDown: 0x50,
  Numpad2: 0x50,
  PageDown: 0x51,
  Numpad3: 0x51,
  Insert: 0x52,
  Numpad0: 0x52,
  Delete: 0x53,
  NumpadDecimal: 0x53,
  IntlBackslash: 0x56,
  F11: 0x57,
  F12: 0x58,
};

/** The scancode of a pressed key, `null` for keys DOS doesn't have. */
export function scancodeFor(e: Pick<KeyboardEvent, "key" | "code">): number | null {
  if (/^[a-z]$/i.test(e.key)) return LETTERS[e.key.toLowerCase()];
  return CODES[e.code] ?? null;
}

/** Esc, Enter and the F-keys, which the games use themselves. */
export function isReserved(scancode: number): boolean {
  return (
    scancode === 0x01 ||
    scancode === 0x1c ||
    (scancode >= 0x3b && scancode <= 0x44) ||
    scancode === 0x57 ||
    scancode === 0x58
  );
}

const NAMED: Record<number, string> = {
  0x01: "esc",
  0x02: "1",
  0x03: "2",
  0x04: "3",
  0x05: "4",
  0x06: "5",
  0x07: "6",
  0x08: "7",
  0x09: "8",
  0x0a: "9",
  0x0b: "0",
  0x0c: "-",
  0x0d: "=",
  0x0e: "backspace",
  0x0f: "tab",
  0x1a: "[",
  0x1b: "]",
  0x1c: "enter",
  0x1d: "ctrl",
  0x27: ";",
  0x28: "'",
  0x29: "`",
  0x2a: "shiftLeft",
  0x2b: "\\",
  0x33: ",",
  0x34: ".",
  0x35: "/",
  0x36: "shiftRight",
  0x37: "num*",
  0x38: "alt",
  0x39: "space",
  0x3a: "capsLock",
  0x3b: "f1",
  0x3c: "f2",
  0x3d: "f3",
  0x3e: "f4",
  0x3f: "f5",
  0x40: "f6",
  0x41: "f7",
  0x42: "f8",
  0x43: "f9",
  0x44: "f10",
  0x45: "numLock",
  0x46: "scrollLock",
  0x47: "home",
  0x48: "up",
  0x49: "pageUp",
  0x4a: "num-",
  0x4b: "left",
  0x4c: "num5",
  0x4d: "right",
  0x4e: "num+",
  0x4f: "end",
  0x50: "down",
  0x51: "pageDown",
  0x52: "insert",
  0x53: "delete",
  0x56: "<",
  0x57: "f11",
  0x58: "f12",
};

const LETTER_IDS = Object.fromEntries(
  Object.entries(LETTERS).map(([letter, scancode]) => [scancode, letter]),
);

/** How a key is called on a keycap. */
export function keyId(scancode: number): string {
  return NAMED[scancode] ?? LETTER_IDS[scancode] ?? `#${scancode.toString(16)}`;
}

/** The keys the games start with. */
export const DEFAULT_KEYS: Record<keyof GameKeys, number> = {
  up: 0x48,
  down: 0x50,
  left: 0x4b,
  right: 0x4d,
  jump: 0x1d,
  pogo: 0x38,
  fire: 0x39,
};

/**
 * Whether `scancode` is what another action starts with in the games, as long
 * as that action hasn't been moved.
 */
export function clashesWithDefault(
  action: keyof GameKeys,
  scancode: number,
  keys: GameKeys,
): boolean {
  return (Object.keys(DEFAULT_KEYS) as (keyof GameKeys)[]).some(
    (other) => other !== action && DEFAULT_KEYS[other] === scancode && keys[other] === null,
  );
}
