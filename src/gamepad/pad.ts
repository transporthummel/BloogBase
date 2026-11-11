/**
 * Turns a standard-mapping pad's state, frame by frame, into what the app
 * does with it (plans/app-controller.md). Pure: the loop in `usePad` feeds
 * it `navigator.getGamepads()`, the tests feed it made-up states.
 */

export type Direction = "up" | "down" | "left" | "right";

/**
 * The face, shoulder and menu buttons beyond A and B are named after the
 * button: what they do depends on what is open (the on-screen keyboard).
 */
export type PadButton = "x" | "y" | "lb" | "rb" | "lt" | "l3" | "menu";

export type PadAction = Direction | "activate" | "back" | PadButton;

/** What the loop needs of a `Gamepad`. */
export type PadState = {
  buttons: readonly { pressed: boolean }[];
  axes: readonly number[];
};

export type PadFrame = {
  actions: PadAction[];
  /** Right stick, -1 to 1 each, 0 inside the dead zone. */
  scroll: { x: number; y: number };
};

// Standard mapping (w3.org/TR/gamepad/#remapping).
const A = 0;
const B = 1;
const BUTTONS: [number, PadButton][] = [
  [2, "x"],
  [3, "y"],
  [4, "lb"],
  [5, "rb"],
  [6, "lt"],
  [9, "menu"],
  [10, "l3"],
];
/** Held, these repeat like a direction (Backspace, caret). */
const REPEATING = new Set<PadButton>(["x", "lb", "rb"]);
const DPAD: [number, Direction][] = [
  [12, "up"],
  [13, "down"],
  [14, "left"],
  [15, "right"],
];

/** A held direction repeats after this long… */
export const REPEAT_DELAY = 400;
/** …then this often. */
export const REPEAT_EVERY = 80;
export const STICK_DEAD_ZONE = 0.5;
export const SCROLL_DEAD_ZONE = 0.2;

/** The left stick's direction: the larger axis wins, nothing in the dead zone. */
export function stickDirection(x: number, y: number): Direction | null {
  if (Math.max(Math.abs(x), Math.abs(y)) < STICK_DEAD_ZONE) return null;
  if (Math.abs(x) > Math.abs(y)) return x < 0 ? "left" : "right";
  return y < 0 ? "up" : "down";
}

function pressed(pad: PadState, i: number) {
  return pad.buttons[i]?.pressed ?? false;
}

/** The D-pad before the stick; with two D-pad directions, the first held. */
function heldDirection(pad: PadState): Direction | null {
  const dpad = DPAD.find(([i]) => pressed(pad, i));
  if (dpad) return dpad[1];
  return stickDirection(pad.axes[0] ?? 0, pad.axes[1] ?? 0);
}

function scrollAxis(v: number) {
  return Math.abs(v) < SCROLL_DEAD_ZONE ? 0 : v;
}

/**
 * Returns a function to call once per frame with the pad (or `null` when
 * none is there or the window has no focus) and the time in ms. Buttons
 * count on the frame they go down; a button already down on the first frame
 * counts too, since the first press is what makes the pad show up at all.
 */
export function padReader() {
  let before: PadState | null = null;
  let held: { dir: Direction; next: number } | null = null;
  const heldButtons = new Map<number, number>();

  return (pad: PadState | null, now: number): PadFrame => {
    const actions: PadAction[] = [];
    if (!pad) {
      before = null;
      held = null;
      heldButtons.clear();
      return { actions, scroll: { x: 0, y: 0 } };
    }

    const went = (i: number) => pressed(pad, i) && !(before && pressed(before, i));

    const dir = heldDirection(pad);
    if (!dir) {
      held = null;
    } else if (held?.dir !== dir) {
      held = { dir, next: now + REPEAT_DELAY };
      actions.push(dir);
    } else if (now >= held.next) {
      held.next = now + REPEAT_EVERY;
      actions.push(dir);
    }
    if (went(A)) actions.push("activate");
    if (went(B)) actions.push("back");
    for (const [i, name] of BUTTONS) {
      if (!pressed(pad, i)) {
        heldButtons.delete(i);
      } else if (went(i)) {
        actions.push(name);
        if (REPEATING.has(name)) heldButtons.set(i, now + REPEAT_DELAY);
      } else {
        const next = heldButtons.get(i);
        if (next !== undefined && now >= next) {
          heldButtons.set(i, now + REPEAT_EVERY);
          actions.push(name);
        }
      }
    }

    before = { buttons: pad.buttons.map((b) => ({ pressed: b.pressed })), axes: [...pad.axes] };
    return {
      actions,
      scroll: { x: scrollAxis(pad.axes[2] ?? 0), y: scrollAxis(pad.axes[3] ?? 0) },
    };
  };
}
