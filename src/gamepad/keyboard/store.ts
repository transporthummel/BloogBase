import type { PadAction } from "../pad";
import type { Field } from "./edit";

/**
 * Which field the on-screen keyboard is open for, if any. `activate` opens
 * it; while it's open, the controller's buttons go to it first (`usePad`).
 */

let field: Field | null = null;
let handler: ((a: PadAction) => void) | null = null;
const listeners = new Set<() => void>();

function changed() {
  for (const l of listeners) l();
}

export function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function keyboardField() {
  return field;
}

/** Fields typed into with the on-screen keyboard: text-like inputs and text areas. */
export function takesKeyboard(el: Element): el is Field {
  if (el instanceof HTMLTextAreaElement) return !el.readOnly;
  return (
    el instanceof HTMLInputElement &&
    !el.readOnly &&
    ["text", "search", "url", "email", "tel", "number", ""].includes(el.type)
  );
}

export function openKeyboard(f: Field) {
  if (field === f) return;
  field = f;
  changed();
}

export function closeKeyboard() {
  if (!field) return;
  field = null;
  changed();
}

/** Set by the rendered keyboard, which knows its own keys. */
export function setKeyboardHandler(h: ((a: PadAction) => void) | null) {
  handler = h;
}

/** Hands a button to the keyboard; false while it's closed. */
export function keyboardAction(a: PadAction): boolean {
  if (!field || !handler) return false;
  handler(a);
  return true;
}
