import { isMac } from "../platform";

export const arrows: Record<string, string> = { left: "←", right: "→", up: "↑", down: "↓" };
// Keys whose name is a word; the rest (letters, digits, F-keys) show as they are printed.
const named = new Set([
  "ctrl",
  "alt",
  "space",
  "enter",
  "esc",
  "tab",
  "shiftLeft",
  "shiftRight",
  "cmd",
  "backspace",
  "capsLock",
  "numLock",
  "scrollLock",
  "home",
  "end",
  "pageUp",
  "pageDown",
  "insert",
  "delete",
]);

// Named differently on Mac keyboards, under `controls.keyMac`; a Mac laptop
// reaches the keys it lacks with fn.
const macNamed = new Set([
  "ctrl",
  "alt",
  "enter",
  "backspace",
  "delete",
  "home",
  "end",
  "pageUp",
  "pageDown",
]);

/** A key's name as printed on it, or spelled out where it's a word. */
export function keyLabel(id: string, t: (key: string) => string) {
  if (arrows[id]) return arrows[id];
  if (!named.has(id)) return id.toUpperCase();
  return t(isMac() && macNamed.has(id) ? `controls.keyMac.${id}` : `controls.key.${id}`);
}
