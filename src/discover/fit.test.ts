import { describe, expect, it } from "vitest";
import type { IndexEntry } from "../bindings";
import { NO_GAME, isSourceMod, listedUnder, needsNoGame } from "./fit";

const entry = (extra: Partial<IndexEntry>, programs: Record<string, string> = {}) =>
  ({
    game: "keen4",
    standalone: false,
    requiresGame: false,
    downloads: [{ programs }],
    ...extra,
  }) as IndexEntry;

describe("source mods and mods with all of their game's files", () => {
  it("lists a source mod under no game alone", () => {
    const source = entry({ standalone: true }, { "FITF.EXE": "ab" });
    expect([needsNoGame(source), isSourceMod(source)]).toEqual([true, true]);
    expect([listedUnder(source, NO_GAME), listedUnder(source, "keen4")]).toEqual([true, false]);
  });

  it("lists a Keen 4 mod that needs no copy under Keen 4 too", () => {
    const complete = entry({ standalone: true });
    expect([needsNoGame(complete), isSourceMod(complete)]).toEqual([true, false]);
    expect([listedUnder(complete, NO_GAME), listedUnder(complete, "keen4")]).toEqual([true, true]);
  });

  it("lists one that needs a copy of any version under its game alone", () => {
    const k6 = entry({ game: "keen6", standalone: true, requiresGame: true });
    expect([needsNoGame(k6), isSourceMod(k6)]).toEqual([false, false]);
    expect([listedUnder(k6, NO_GAME), listedUnder(k6, "keen6")]).toEqual([false, true]);
  });
});
