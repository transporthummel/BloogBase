import { expect, test } from "vitest";
import { clashesWithDefault, isReserved, keyId, scancodeFor } from "./scancodes";

const noKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};

test.each([
  // A letter goes by what's printed on the key: the German Z sits where US has Y.
  [{ key: "z", code: "KeyY" }, 0x2c],
  [{ key: "Z", code: "KeyY" }, 0x2c],
  [{ key: "ArrowLeft", code: "ArrowLeft" }, 0x4b],
  [{ key: "4", code: "Numpad4" }, 0x4b],
  [{ key: "Control", code: "ControlRight" }, 0x1d],
  [{ key: "AltGraph", code: "AltRight" }, 0x38],
  [{ key: " ", code: "Space" }, 0x39],
  [{ key: "ü", code: "BracketLeft" }, 0x1a],
  [{ key: "Dead", code: "Equal" }, 0x0d],
  [{ key: "Meta", code: "MetaLeft" }, null],
  [{ key: "F13", code: "F13" }, null],
  [{ key: "Unidentified", code: "" }, null],
])("scancodeFor(%o) is %s", (event, scancode) => {
  expect(scancodeFor(event)).toBe(scancode);
});

test("the keys the games use themselves are reserved", () => {
  for (const code of ["Escape", "Enter", "NumpadEnter", "F1", "F10", "F11", "F12"]) {
    expect(isReserved(scancodeFor({ key: "", code })!), code).toBe(true);
  }
  expect(isReserved(0x39)).toBe(false);
});

test("every key that can be chosen has an id", () => {
  expect(keyId(0x39)).toBe("space");
  expect(keyId(0x1d)).toBe("ctrl");
  expect(keyId(0x48)).toBe("up");
  expect(keyId(0x11)).toBe("w");
  for (let scancode = 1; scancode < 0x59; scancode++) {
    if (scancode >= 0x54 && scancode <= 0x55) continue;
    expect(keyId(scancode), `${scancode}`).toBeTruthy();
  }
});

test("a key clashes with another action's default unless that one moved", () => {
  expect(clashesWithDefault("jump", 0x39, noKeys)).toBe(true);
  expect(clashesWithDefault("fire", 0x39, noKeys)).toBe(false);
  expect(clashesWithDefault("jump", 0x39, { ...noKeys, fire: 0x11 })).toBe(false);
  expect(clashesWithDefault("jump", 0x11, noKeys)).toBe(false);
});
