import type { Direction } from "./pad";
import { openKeyboard, takesKeyboard } from "./keyboard/store";
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

/** Sends a key, marked as the controller's; true if a handler took it. */
export function sendKey(target: Element, key: string): boolean {
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
 * A: clicks the focused element, steps a `<select>`, opens the on-screen
 * keyboard for a text field. With nothing in
 * particular focused (the page, a dialog), Enter, for a page's default
 * action (`useEnterAction`).
 */
export function activate(doc: Document = document) {
  markPadInput(doc);
  const el = doc.activeElement;
  if (el instanceof HTMLSelectElement) return stepSelect(el);
  if (el && takesKeyboard(el)) return openKeyboard(el);
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

function canScroll(el: Element) {
  const { overflowY } = getComputedStyle(el);
  return (overflowY === "auto" || overflowY === "scroll") && el.scrollHeight > el.clientHeight;
}

/**
 * What the right stick and the triggers scroll: the scroll container around
 * the focus, else the open dialog's, else the page's (`<main>`).
 */
export function scroller(doc: Document = document): HTMLElement | null {
  const scope = padScope(doc);
  for (let el = doc.activeElement; el && el !== doc.body; el = el.parentElement) {
    if (el instanceof HTMLElement && canScroll(el) && scope.contains(el)) return el;
  }
  if (scope !== doc.body) {
    const inner = [scope, ...scope.querySelectorAll<HTMLElement>("*")].find(canScroll);
    if (inner) return inner;
  }
  return doc.querySelector("main");
}

/** The tab list LB/RB switch: in the scope, the one holding the focus first. */
function tabList(doc: Document): HTMLElement | null {
  const scope = padScope(doc);
  const lists = [...scope.querySelectorAll<HTMLElement>('[role="tablist"]')];
  const active = doc.activeElement;
  return lists.find((l) => active && l.contains(active)) ?? lists[0] ?? null;
}

/**
 * LB/RB: the previous or next tab, wrapping, as the tab list's own arrow
 * keys do (`Tabs`). The focus follows into the new tab only if it was in
 * the tab list or the panel going away; elsewhere (the Play button) it stays.
 */
export function switchTab(dir: -1 | 1, doc: Document = document) {
  markPadInput(doc);
  const list = tabList(doc);
  const tab = list?.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');
  if (!list || !tab) return;
  const active = doc.activeElement;
  const panel = doc.getElementById(tab.getAttribute("aria-controls") ?? "");
  const follows = !!active && (list.contains(active) || !!panel?.contains(active));
  sendKey(tab, dir < 0 ? "ArrowLeft" : "ArrowRight");
  if (!follows && active instanceof HTMLElement && active !== doc.body) {
    active.focus({ preventScroll: true });
  }
  list.scrollIntoView?.({ block: "nearest" });
}

/** How much of a page scrolls on LT/RT: a little stays in sight. */
export const PAGE_SHARE = 0.9;

/**
 * LT/RT: scrolls a page up or down. If the focus leaves the view, it moves
 * to the first element in view in that direction (the top one going down),
 * so the D-pad carries on from what's on screen.
 */
export function scrollPage(dir: -1 | 1, doc: Document = document) {
  markPadInput(doc);
  const box = scroller(doc);
  if (!box) return;
  const before = box.scrollTop;
  const max = box.scrollHeight - box.clientHeight;
  const top = Math.min(max, Math.max(0, before + dir * box.clientHeight * PAGE_SHARE));
  const by = top - before;
  if (by === 0) return;
  box.scrollTo({ top, behavior: "smooth" });

  // Where things will be once it has scrolled.
  const view = box.getBoundingClientRect();
  const inView = (el: Element) => {
    const r = el.getBoundingClientRect();
    return r.top - by >= view.top && r.bottom - by <= view.bottom;
  };
  const active = doc.activeElement;
  if (active && active !== doc.body && box.contains(active) && inView(active)) return;
  const shown = candidates(box).filter(inView);
  const rect = (el: Element) => el.getBoundingClientRect();
  const next =
    dir > 0
      ? shown.sort((a, b) => rect(a).top - rect(b).top || rect(a).left - rect(b).left)[0]
      : shown.sort((a, b) => rect(b).bottom - rect(a).bottom || rect(a).left - rect(b).left)[0];
  next?.focus({ preventScroll: true });
}

/** Where View/Menu came from, to go back there. */
let beforeNav: HTMLElement | null = null;

/**
 * View/Menu: to the sidebar's item for the current page; pressed again
 * there, back where the focus was. Nothing while a dialog or menu is open
 * (the sidebar is behind it) or without a sidebar (the setup).
 */
export function toggleNav(doc: Document = document) {
  markPadInput(doc);
  if (padScope(doc) !== doc.body) return;
  const nav = doc.querySelector<HTMLElement>("nav");
  if (!nav) return;
  const active = doc.activeElement;
  if (active && nav.contains(active)) {
    const back = beforeNav;
    const target = back?.isConnected && isCandidate(back) ? back : firstIn(doc.body);
    if (target) focusForPad(target);
    return;
  }
  beforeNav = active instanceof HTMLElement && active !== doc.body ? active : null;
  const items = candidates(nav);
  const current = items.find((el) => el.getAttribute("aria-current") === "page");
  const target = current ?? items[0];
  if (target) focusForPad(target);
}
