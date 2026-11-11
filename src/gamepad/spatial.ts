import type { Direction } from "./pad";

/**
 * Spatial focus for the controller: from the focused element to the nearest
 * one in a direction, like Edge on the Xbox (PLAN.md §15). What lies in line
 * with the focus (overlaps it across the direction) comes first, as on
 * Android: down from a link above the tabs goes to the tabs, not to the
 * sidebar beside it that is nearer as the crow flies.
 */

const FOCUSABLE =
  'a[href], button, input:not([type="hidden"]), select, textarea, summary, [tabindex], [contenteditable="true"]';

/**
 * How much of a candidate may lie behind the edge we leave, as a share of
 * its size: a wide button half beside the focus still counts as below it.
 */
const OVERLAP = 0.3;

/** Whether the controller can move the focus to `el`. */
export function isCandidate(el: Element): el is HTMLElement {
  if (!(el instanceof HTMLElement) || !el.matches(FOCUSABLE)) return false;
  if (el.tabIndex < 0 || el.matches(":disabled")) return false;
  if (el.closest("[inert], [data-pad-ignore]")) return false;
  const r = el.getBoundingClientRect();
  if (r.width === 0 && r.height === 0) return false;
  return getComputedStyle(el).visibility !== "hidden";
}

export function candidates(scope: Element): HTMLElement[] {
  return [...scope.querySelectorAll(FOCUSABLE)].filter(isCandidate);
}

/**
 * Where the controller may move: an open menu, else the innermost and last
 * open dialog, else the whole page. Without it the focus would wander behind
 * a dialog.
 */
export function padScope(doc: Document = document): HTMLElement {
  const menu = doc.querySelector<HTMLElement>('[role="menu"]');
  if (menu) return menu;
  const dialogs = [...doc.querySelectorAll<HTMLElement>('[role="dialog"]')].filter(
    (d) => !d.querySelector('[role="dialog"]'),
  );
  return dialogs[dialogs.length - 1] ?? doc.body;
}

type Point = { x: number; y: number };

function exitPoint(r: DOMRect, dir: Direction): Point {
  switch (dir) {
    case "left":
      return { x: r.left, y: (r.top + r.bottom) / 2 };
    case "right":
      return { x: r.right, y: (r.top + r.bottom) / 2 };
    case "up":
      return { x: (r.left + r.right) / 2, y: r.top };
    case "down":
      return { x: (r.left + r.right) / 2, y: r.bottom };
  }
}

/** Whether `r` lies in `dir` from `p`, all but `OVERLAP` of it. */
function inDirection(r: DOMRect, dir: Direction, p: Point) {
  switch (dir) {
    case "left":
      return r.left + r.width * (1 - OVERLAP) < p.x;
    case "right":
      return r.left + r.width * OVERLAP > p.x;
    case "up":
      return r.top + r.height * (1 - OVERLAP) < p.y;
    case "down":
      return r.top + r.height * OVERLAP > p.y;
  }
}

/** Whether score `a` beats `b`: the first entry that differs decides. */
function before(a: number[], b: number[]) {
  const i = a.findIndex((v, j) => v !== b[j]);
  return i >= 0 && a[i] < b[i];
}

/** Gap along the direction, and across it (0 where they overlap). */
function gaps(f: DOMRect, r: DOMRect, dir: Direction) {
  const across =
    dir === "up" || dir === "down"
      ? Math.max(0, r.left - f.right, f.left - r.right)
      : Math.max(0, r.top - f.bottom, f.top - r.bottom);
  const along = Math.max(
    0,
    {
      up: f.top - r.bottom,
      down: r.top - f.bottom,
      left: f.left - r.right,
      right: r.left - f.right,
    }[dir],
  );
  const overlap =
    dir === "up" || dir === "down"
      ? Math.min(f.right, r.right) - Math.max(f.left, r.left)
      : Math.min(f.bottom, r.bottom) - Math.max(f.top, r.top);
  const centres =
    dir === "up" || dir === "down"
      ? Math.abs(f.left + f.width / 2 - (r.left + r.width / 2))
      : Math.abs(f.top + f.height / 2 - (r.top + r.height / 2));
  return { along, across, inLine: overlap > 0, centres };
}

/**
 * The nearest candidate in `dir` from `from` inside `scope`, if any: in line
 * with it the nearest along the direction, else the nearest at all, a step
 * across counting twice.
 */
export function nextFocus(from: Element, dir: Direction, scope: Element): HTMLElement | null {
  const f = from.getBoundingClientRect();
  const p = exitPoint(f, dir);
  let best: HTMLElement | null = null;
  let bestScore: number[] = [];
  for (const el of candidates(scope)) {
    if (el === from || el.contains(from)) continue;
    const r = el.getBoundingClientRect();
    if (!inDirection(r, dir, p)) continue;
    const g = gaps(f, r, dir);
    const score = g.inLine
      ? [0, g.along, g.centres]
      : [1, Math.hypot(g.along, g.across) + g.across, g.centres];
    if (!best || before(score, bestScore)) {
      best = el;
      bestScore = score;
    }
  }
  return best;
}
