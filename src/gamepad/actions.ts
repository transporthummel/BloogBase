import type { Direction } from "./pad";
import { candidates, isCandidate, nextFocus, padScope } from "./spatial";

/**
 * What the controller's buttons do to the page. Keys made by a script move
 * no focus and click nothing, so this moves and clicks itself; our own key
 * handlers (dialogs, menus, tabs) see the keys it sends.
 */

const KEYS: Record<Direction, string> = {
  up: "ArrowUp",
  down: "ArrowDown",
  left: "ArrowLeft",
  right: "ArrowRight",
};

/** Elements a click does something on, even without a tab stop (menu items). */
const INTERACTIVE = "a[href], button, input, select, textarea, summary";

const padKeys = new WeakSet<Event>();

/** Whether a key came from the controller rather than a keyboard. */
export function isPadKey(e: Event) {
  return padKeys.has(e);
}

function sendKey(target: Element, key: string): boolean {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  padKeys.add(e);
  target.dispatchEvent(e);
  return e.defaultPrevented;
}

/** Fields take the arrow keys for the caret, which a sent key can't move anyway. */
function isEditable(el: Element) {
  return (
    (el instanceof HTMLInputElement &&
      !["checkbox", "radio", "button", "submit"].includes(el.type)) ||
    el instanceof HTMLTextAreaElement ||
    (el instanceof HTMLElement && el.isContentEditable)
  );
}

/** Marks controller use, so the focus ring shows (index.css); the mouse clears it. */
export function markPadInput(doc: Document = document) {
  doc.documentElement.dataset.input = "pad";
}

export function focusForPad(el: HTMLElement) {
  markPadInput(el.ownerDocument);
  el.focus({ preventScroll: true, focusVisible: true });
  el.scrollIntoView?.({ block: "nearest", inline: "nearest" });
}

/** The first candidate in view, else the first at all; on the page, in `<main>`. */
function firstIn(scope: HTMLElement): HTMLElement | null {
  const doc = scope.ownerDocument;
  const area = scope === doc.body ? (doc.querySelector("main") ?? scope) : scope;
  const all = candidates(area);
  const height = doc.defaultView?.innerHeight ?? 0;
  return (
    all.find((el) => {
      const r = el.getBoundingClientRect();
      return r.bottom > 0 && r.top < height;
    }) ??
    all[0] ??
    null
  );
}

/**
 * D-pad or left stick. A component with its own arrow keys (tabs, menus)
 * gets the key first; if it doesn't take it, the focus moves to the nearest
 * element in that direction. With nothing focused yet, to the first.
 */
export function move(dir: Direction, doc: Document = document) {
  markPadInput(doc);
  const active = doc.activeElement;
  if (active && active !== doc.body && !isEditable(active) && sendKey(active, KEYS[dir])) return;
  const scope = padScope(doc);
  const from = active && isCandidate(active) ? active : null;
  const target =
    (from && nextFocus(from, dir, scope)) ?? (from && scope.contains(from) ? null : firstIn(scope));
  if (target) focusForPad(target);
}

/** The next option of a `<select>`, after the last the first, as a change. */
export function stepSelect(select: HTMLSelectElement) {
  const n = select.options.length;
  for (let i = 1; i <= n; i++) {
    const at = (select.selectedIndex + i) % n;
    if (!select.options[at].disabled) {
      if (at === select.selectedIndex) return;
      select.selectedIndex = at;
      select.dispatchEvent(new Event("input", { bubbles: true }));
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
  }
}

/**
 * A: clicks the focused element, steps a `<select>`. With nothing in
 * particular focused (the page, a dialog), Enter, for a page's default
 * action (`useEnterAction`).
 */
export function activate(doc: Document = document) {
  markPadInput(doc);
  const el = doc.activeElement;
  if (el instanceof HTMLSelectElement) return stepSelect(el);
  if (
    el instanceof HTMLElement &&
    el !== doc.body &&
    (el.matches(INTERACTIVE) || el.tabIndex >= 0)
  ) {
    return el.click();
  }
  sendKey(el ?? doc.body, "Enter");
}

/**
 * B: Escape while a dialog or menu is open, else `goBack` (a page back, as
 * far as there is one to go to).
 */
export function back(goBack: () => void, doc: Document = document) {
  markPadInput(doc);
  if (doc.querySelector('[role="dialog"], [role="menu"]')) {
    sendKey(doc.activeElement ?? doc.body, "Escape");
  } else {
    goBack();
  }
}
