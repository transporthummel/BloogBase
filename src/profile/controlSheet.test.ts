import { expect, test } from "vitest";
import type { GameKeys } from "../bindings";
import { keysInEffect, sheetFor } from "./controlSheet";

const noKeys: GameKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};
const keen1 = { id: "keen1", engine: "vorticons" } as const;
const keen4 = { id: "keen4", engine: "galaxy" } as const;
const dreams = { id: "kdreams", engine: "dreams" } as const;

const action = (sheet: ReturnType<typeof sheetFor>, name: string) =>
  sheet.actions.find((b) => b.action === name);

test("set keys replace the defaults and are marked", () => {
  const sheet = sheetFor(keen4, false, false, { ...noKeys, jump: 0x11, left: 0x1e });
  expect(action(sheet, "jump")).toEqual({ keys: ["w"], action: "jump", changed: true });
  expect(action(sheet, "pogo")).toEqual({ keys: ["alt"], action: "pogo" });
  expect(sheet.moving.left).toEqual({ id: "a", changed: true });
  expect(sheet.moving.right).toEqual({ id: "right", changed: false });
});

test("Keen 1 shoots with the Fire key where the patch fits", () => {
  const sheet = sheetFor(keen1, true, false, { ...noKeys, fire: 0x25 });
  expect(action(sheet, "fire")).toEqual({ keys: ["k"], action: "fire", changed: true });
});

test("without the patch, Keen 1 shoots with Jump and Pogo together", () => {
  const sheet = sheetFor(keen1, false, false, { ...noKeys, jump: 0x11, pogo: 0x12, fire: 0x25 });
  expect(action(sheet, "fire")).toEqual({ keys: ["w", "e"], action: "fire", changed: true });
});

test("Keen Dreams throws with the Fire key and has no pogo", () => {
  const sheet = sheetFor(dreams, false, false, { ...noKeys, pogo: 0x12, fire: 0x25 });
  expect(action(sheet, "throw")).toEqual({ keys: ["k"], action: "throw", changed: true });
  expect(action(sheet, "pogo")).toBeUndefined();
});

test("nothing set shows the games' defaults", () => {
  const moving = {
    up: { id: "up", changed: false },
    down: { id: "down", changed: false },
    left: { id: "left", changed: false },
    right: { id: "right", changed: false },
  };
  const galaxy = sheetFor(keen4, false, false, noKeys);
  expect(galaxy.moving).toEqual(moving);
  expect(galaxy.actions).toEqual([
    { keys: ["ctrl"], action: "jump" },
    { keys: ["alt"], action: "pogo" },
    { keys: ["space"], action: "fire" },
  ]);
  expect(action(sheetFor(keen1, true, false, noKeys), "fire")).toEqual({
    keys: ["space"],
    action: "fire",
    changed: true,
  });
  expect(action(sheetFor(keen1, false, false, noKeys), "fire")).toEqual({
    keys: ["ctrl", "alt"],
    action: "fire",
  });
});

test("the Fire key only shows where it's applied", () => {
  const keys = { ...noKeys, jump: 0x11, fire: 0x25 };
  // One-button firing off, so a mod's own patch keeps Space.
  expect(keysInEffect(keys, keen1, null, false).fire).toBeNull();
  expect(keysInEffect(keys, keen1, null, undefined).fire).toBeNull();
  expect(keysInEffect(keys, keen1, null, true).fire).toBe(0x25);
  // Keen 4's special demo has no such key in its file.
  expect(keysInEffect(keys, keen4, "keen4-demo", false)).toEqual({ ...keys, fire: null });
  expect(keysInEffect(keys, keen4, "keen4-1.4-ega", false)).toEqual(keys);
  expect(keysInEffect(keys, dreams, "kdreams-1.13", false)).toEqual(keys);
});
