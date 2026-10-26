import { expect, test } from "vitest";
import { isChristmas } from "./christmas";

test("snows from Christmas Eve to New Year's Day", () => {
  expect(isChristmas(new Date(2026, 11, 23, 23, 59))).toBe(false);
  expect(isChristmas(new Date(2026, 11, 24, 0, 0))).toBe(true);
  expect(isChristmas(new Date(2026, 11, 31, 23, 59))).toBe(true);
  expect(isChristmas(new Date(2027, 0, 1, 23, 59))).toBe(true);
  expect(isChristmas(new Date(2027, 0, 2))).toBe(false);
  expect(isChristmas(new Date(2026, 6, 24))).toBe(false);
});
