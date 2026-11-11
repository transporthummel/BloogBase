import { describe, expect, it } from "vitest";
import { clamp, layout, nextShift, step, typed, type Key, type Layout } from "./layout";

const chars = (keys: Layout) =>
  keys.flat().flatMap((k: Key) => (k.type === "char" ? [k.char] : []));

const width = (row: Key[]) => row.reduce((s, k) => s + k.width, 0);

describe("layout", () => {
  it("is QWERTZ with umlauts in German, QWERTY in English", () => {
    const de = layout("de", "letters");
    expect(de[1].map((k) => (k.type === "char" ? k.char : "")).join("")).toBe("qwertzuiopü");
    expect(chars(de)).toEqual(expect.arrayContaining(["ä", "ö", "ü", "ß"]));
    const en = layout("en", "letters");
    expect(en[1].map((k) => (k.type === "char" ? k.char : "")).join("")).toBe("qwertyuiop");
    expect(chars(en)).not.toContain("ä");
  });

  it("has every key once per page", () => {
    for (const lang of ["de", "en"]) {
      for (const page of ["letters", "symbols", "digits"] as const) {
        const all = chars(layout(lang, page));
        expect(new Set(all).size).toBe(all.length);
      }
    }
  });

  it("puts all of printable ASCII's symbols on the symbols page", () => {
    const all = chars(layout("en", "symbols"));
    for (let c = 33; c < 127; c++) {
      const char = String.fromCharCode(c);
      if (!/[a-z0-9]/i.test(char)) expect(all).toContain(char);
    }
  });

  it("ends with Shift, symbols, Space, Backspace and Done, no wider than the rows", () => {
    const keys = layout("de", "letters");
    const bottom = keys[keys.length - 1];
    expect(bottom.map((k) => (k.type === "action" ? k.action : ""))).toEqual([
      "shift",
      "symbols",
      "space",
      "backspace",
      "done",
    ]);
    expect(width(bottom)).toBeLessThanOrEqual(Math.max(...keys.slice(0, -1).map(width)));
  });

  it("has only digits, Backspace and Done for number fields", () => {
    const keys = layout("de", "digits");
    expect(chars(keys).sort().join("")).toBe("0123456789");
    expect(
      keys
        .flat()
        .filter((k) => k.type === "action")
        .map((k) => k.type === "action" && k.action),
    ).toEqual(["backspace", "done"]);
  });
});

describe("step", () => {
  const keys = layout("en", "letters");

  it("wraps left and right within a row", () => {
    expect(step(keys, { row: 1, col: 0 }, "left")).toEqual({ row: 1, col: 9 });
    expect(step(keys, { row: 1, col: 9 }, "right")).toEqual({ row: 1, col: 0 });
  });

  it("stops at the top and bottom", () => {
    expect(step(keys, { row: 0, col: 3 }, "up")).toEqual({ row: 0, col: 3 });
    expect(step(keys, { row: 4, col: 2 }, "down")).toEqual({ row: 4, col: 2 });
  });

  it("goes to the nearest key in the next row, rows being centred", () => {
    // Q over A: the shorter row sits half a key in.
    expect(step(keys, { row: 1, col: 0 }, "down")).toEqual({ row: 2, col: 0 });
    // V down to the wide Space, Space up to the key over its middle.
    expect(step(keys, { row: 3, col: 3 }, "down")).toEqual({ row: 4, col: 2 });
    expect(step(keys, { row: 4, col: 2 }, "up")).toEqual({ row: 3, col: 4 });
    expect(step(keys, { row: 4, col: 4 }, "up")).toEqual({ row: 3, col: 9 });
  });

  it("keeps the place on another page as far as it goes", () => {
    expect(clamp(layout("en", "digits"), { row: 4, col: 4 })).toEqual({ row: 4, col: 1 });
  });
});

describe("shift", () => {
  it("goes once, locked, off", () => {
    expect(nextShift("off")).toBe("once");
    expect(nextShift("once")).toBe("lock");
    expect(nextShift("lock")).toBe("off");
  });

  it("capitalises letters but leaves ß and symbols", () => {
    expect(typed("ä", "once")).toBe("Ä");
    expect(typed("ß", "lock")).toBe("ß");
    expect(typed("-", "lock")).toBe("-");
    expect(typed("q", "off")).toBe("q");
  });
});
