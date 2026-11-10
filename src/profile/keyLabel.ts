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

/** A key's name as printed on it, or spelled out where it's a word. */
export function keyLabel(id: string, t: (key: string) => string) {
  return arrows[id] ?? (named.has(id) ? t(`controls.key.${id}`) : id.toUpperCase());
}
