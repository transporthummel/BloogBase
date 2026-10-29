import { type TFunction } from "i18next";
import { expect, test } from "vitest";
import { formatLooseDate, formatPlayTime, formatRelative } from "./format";

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

test("KeenWiki dates are shown as precisely as written, in the user's language", () => {
  expect(formatLooseDate("de", "2017-07-04")).toBe("4. Juli 2017");
  expect(formatLooseDate("en", "2017-07-04")).toBe("Jul 4, 2017");
  expect(formatLooseDate("de", "November 22, 1991[1]")).toBe("22. Nov. 1991");
  expect(formatLooseDate("de", "22 November 1991")).toBe("22. Nov. 1991");
  expect(formatLooseDate("de", "November 1991")).toBe("November 1991");
  expect(formatLooseDate("de", "2009-10")).toBe("Oktober 2009");
  expect(formatLooseDate("de", "2009")).toBe("2009");
  expect(formatLooseDate("de", "2017-02-30")).toBe("2017-02-30");
  expect(formatLooseDate("de", "Late 2005")).toBe("Late 2005");
});

test("play time says 0 min when nothing was played, not less than a minute", () => {
  const counted = ((key: string, o?: { count?: number }) =>
    o?.count === undefined ? key : `${key}:${o.count}`) as unknown as TFunction;
  expect(formatPlayTime(counted, 0)).toBe("time.minutes:0");
  expect(formatPlayTime(counted, 30)).toBe("time.lessThanMinute");
  expect(formatPlayTime(counted, 720)).toBe("time.minutes:12");
});
