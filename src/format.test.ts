import { type TFunction } from "i18next";
import { expect, test } from "vitest";
import { formatRelative } from "./format";

const t = ((key: string) => key) as unknown as TFunction;
const now = new Date(2026, 9, 21, 14, 0).getTime();
const ago = (then: Date) => formatRelative(t, "de", then.getTime() / 1000, now);

test("relative times count minutes, hours and calendar days", () => {
  expect(ago(new Date(2026, 9, 21, 13, 59, 30))).toBe("time.justNow");
  expect(ago(new Date(2026, 9, 21, 13, 55))).toBe("vor 5 Minuten");
  expect(ago(new Date(2026, 9, 21, 2, 0))).toBe("vor 12 Stunden");
  expect(ago(new Date(2026, 9, 20, 23, 0))).toBe("gestern");
  expect(ago(new Date(2026, 9, 18, 9, 0))).toBe("vor 3 Tagen");
  expect(ago(new Date(2026, 9, 14, 9, 0))).toBe("vor 1 Woche");
  expect(ago(new Date(2026, 8, 25, 9, 0))).toBe("vor 3 Wochen");
  expect(ago(new Date(2026, 8, 10, 9, 0))).toBe("vor 1 Monat");
  expect(ago(new Date(2025, 9, 22, 9, 0))).toBe("vor 11 Monaten");
  expect(ago(new Date(2024, 9, 21, 9, 0))).toBe("vor 2 Jahren");
});

test("shortly after midnight, last night reads in hours", () => {
  const early = new Date(2026, 9, 21, 1, 0).getTime();
  expect(formatRelative(t, "en", new Date(2026, 9, 20, 23, 0).getTime() / 1000, early)).toBe(
    "2 hours ago",
  );
});
