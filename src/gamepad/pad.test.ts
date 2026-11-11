import { describe, expect, it } from "vitest";
import {
  padReader,
  REPEAT_DELAY,
  REPEAT_EVERY,
  REPEAT_SLOW,
  stickDirection,
  type PadState,
} from "./pad";

function pad(down: number[] = [], axes: number[] = [0, 0, 0, 0]): PadState {
  return {
    buttons: Array.from({ length: 17 }, (_, i) => ({ pressed: down.includes(i) })),
    axes,
  };
}

describe("stickDirection", () => {
  it("ignores small deflections", () => {
    expect(stickDirection(0.4, -0.3)).toBeNull();
  });

  it("takes the larger axis, up being negative", () => {
    expect(stickDirection(0.6, -0.9)).toBe("up");
    expect(stickDirection(-0.9, 0.6)).toBe("left");
    expect(stickDirection(0.2, 1)).toBe("down");
    expect(stickDirection(0.7, 0.1)).toBe("right");
  });
});

describe("padReader", () => {
  it("counts a button once, on the frame it goes down", () => {
    const read = padReader();
    expect(read(pad(), 0).actions).toEqual([]);
    expect(read(pad([0]), 16).actions).toEqual(["activate"]);
    expect(read(pad([0]), 32).actions).toEqual([]);
    expect(read(pad(), 48).actions).toEqual([]);
    expect(read(pad([0, 1]), 64).actions).toEqual(["activate", "back"]);
  });

  it("keeps the press that made the pad show up", () => {
    const read = padReader();
    expect(read(pad([0]), 0).actions).toEqual(["activate"]);
  });

  it("forgets the pad while it's gone", () => {
    const read = padReader();
    read(pad([1]), 0);
    expect(read(null, 16).actions).toEqual([]);
    expect(read(pad([1]), 32).actions).toEqual(["back"]);
  });

  it("repeats a held direction after a pause", () => {
    const read = padReader();
    expect(read(pad([13]), 0).actions).toEqual(["down"]);
    expect(read(pad([13]), REPEAT_DELAY - 1).actions).toEqual([]);
    expect(read(pad([13]), REPEAT_DELAY).actions).toEqual(["down"]);
    expect(read(pad([13]), REPEAT_DELAY + REPEAT_EVERY - 1).actions).toEqual([]);
    expect(read(pad([13]), REPEAT_DELAY + REPEAT_EVERY).actions).toEqual(["down"]);
  });

  it("starts over when the direction changes or is let go", () => {
    const read = padReader();
    read(pad([13]), 0);
    expect(read(pad([15]), 100).actions).toEqual(["right"]);
    expect(read(pad([15]), 100 + REPEAT_DELAY - 1).actions).toEqual([]);
    read(pad(), 600);
    expect(read(pad([15]), 616).actions).toEqual(["right"]);
  });

  it("reads the left stick like the D-pad, the D-pad first", () => {
    const read = padReader();
    expect(read(pad([], [0, -1, 0, 0]), 0).actions).toEqual(["up"]);
    expect(read(pad([], [0, -1, 0, 0]), 16).actions).toEqual([]);
    expect(read(pad([14], [0, -1, 0, 0]), 32).actions).toEqual(["left"]);
  });

  it("passes the right stick on as scrolling, outside its dead zone", () => {
    const read = padReader();
    expect(read(pad([], [0, 0, 0.1, -0.6]), 0).scroll).toEqual({ x: 0, y: -0.6 });
    expect(read(pad([], [0, 0, 0, 0]), 16).scroll).toEqual({ x: 0, y: 0 });
  });

  it("names the other buttons, once per press", () => {
    const read = padReader();
    expect(read(pad([2, 3, 4, 5, 6, 7, 8, 9, 10]), 0).actions).toEqual([
      "x",
      "y",
      "lb",
      "rb",
      "lt",
      "rt",
      "view",
      "menu",
      "l3",
    ]);
    expect(read(pad([3, 8, 9, 10]), REPEAT_DELAY).actions).toEqual([]);
  });

  it("counts a trigger from half way", () => {
    const read = padReader();
    const trigger = (value: number) => {
      const p = pad();
      return {
        ...p,
        buttons: p.buttons.map((b, i) => (i === 7 ? { pressed: value > 0, value } : b)),
      };
    };
    expect(read(trigger(0.3), 0).actions).toEqual([]);
    expect(read(trigger(0.6), 16).actions).toEqual(["rt"]);
    expect(read(trigger(0.9), 32).actions).toEqual([]);
    read(trigger(0.4), 48);
    expect(read(trigger(0.7), 64).actions).toEqual(["rt"]);
  });

  it("repeats Backspace (X) fast, shoulders and triggers slower, not the rest", () => {
    const read = padReader();
    read(pad([2, 3]), 0);
    expect(read(pad([2, 3]), REPEAT_DELAY).actions).toEqual(["x"]);
    expect(read(pad([2, 3]), REPEAT_DELAY + REPEAT_EVERY).actions).toEqual(["x"]);
    read(pad(), 1000);
    expect(read(pad([5, 7]), 1016).actions).toEqual(["rb", "rt"]);
    expect(read(pad([5, 7]), 1016 + REPEAT_DELAY).actions).toEqual(["rb", "rt"]);
    expect(read(pad([5, 7]), 1016 + REPEAT_DELAY + REPEAT_EVERY).actions).toEqual([]);
    expect(read(pad([5, 7]), 1016 + REPEAT_DELAY + REPEAT_SLOW).actions).toEqual(["rb", "rt"]);
  });
});
