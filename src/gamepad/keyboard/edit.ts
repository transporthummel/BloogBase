/**
 * Typing into a field from the on-screen keyboard: pure steps on its text
 * and caret, and the one function that puts the result into the field.
 */

export type Text = {
  value: string;
  start: number;
  end: number;
  /** -1 for none, as on a field. */
  maxLength: number;
};

/** Replaces the selection with `text`, as far as `maxLength` allows. */
export function insert(t: Text, text: string): Text {
  const room = t.maxLength < 0 ? Infinity : t.maxLength - (t.value.length - (t.end - t.start));
  const add = [...text].slice(0, Math.max(0, room)).join("");
  const value = t.value.slice(0, t.start) + add + t.value.slice(t.end);
  const at = t.start + add.length;
  return { ...t, value, start: at, end: at };
}

/** Deletes the selection, else the character before the caret. */
export function backspace(t: Text): Text {
  if (t.start !== t.end) return { ...insert(t, "") };
  if (t.start === 0) return t;
  // A whole character, not half a surrogate pair.
  const before = [...t.value.slice(0, t.start)];
  before.pop();
  const head = before.join("");
  return { ...t, value: head + t.value.slice(t.end), start: head.length, end: head.length };
}

/** Moves the caret one character; a selection collapses to its side. */
export function moveCaret(t: Text, dir: -1 | 1): Text {
  if (t.start !== t.end) {
    const at = dir < 0 ? t.start : t.end;
    return { ...t, start: at, end: at };
  }
  const at = Math.min(t.value.length, Math.max(0, t.start + dir));
  return { ...t, start: at, end: at };
}

export type Field = HTMLInputElement | HTMLTextAreaElement;

/** Number and some other fields have no caret to read: they type at the end. */
function selection(field: Field): [number, number] {
  try {
    const { selectionStart, selectionEnd } = field;
    if (selectionStart !== null && selectionEnd !== null) return [selectionStart, selectionEnd];
  } catch {
    // Not supported on this type.
  }
  return [field.value.length, field.value.length];
}

export function readField(field: Field): Text {
  const [start, end] = selection(field);
  return { value: field.value, start, end, maxLength: field.maxLength };
}

/**
 * Writes `t` into the field so React sees it: through the native setter
 * (React's own swallows the change) and an `input` event, then the caret.
 */
export function writeField(field: Field, t: Text) {
  if (t.value !== field.value) {
    const proto =
      field instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype
        : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(field, t.value);
    field.dispatchEvent(new Event("input", { bubbles: true }));
  }
  try {
    field.setSelectionRange(t.start, t.end);
  } catch {
    // No caret on this type.
  }
}
