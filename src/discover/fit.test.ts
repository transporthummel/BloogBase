import { describe, expect, it } from "vitest";
import type { FreeGame, Game, IndexEntry } from "../bindings";
import { NO_GAME, freeFit, isSourceMod, listedUnder, needsNoGame } from "./fit";

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

describe("freeFit", () => {
  const keen4 = {
    id: "keen4",
    versions: [
      { id: "keen4-1.1-ega", original: null },
      { id: "keen4-1.4-ega", original: null },
      { id: "keen4-1.4-cga", original: null },
      { id: "keen4-demo", original: null, kind: "demo" },
    ],
  } as unknown as Game;
  const free = (versionId: string, setup = false, installed = false): FreeGame => ({
    gameId: "keen4",
    versionId,
    setup,
    size: 1,
    installed,
  });
  const all = [free("keen4-demo"), free("keen4-1.4-cga"), free("keen4-1.4-ega", true)];

  it("takes the free version the mod is made for", () => {
    expect(freeFit(["keen4-1.4-ega"], keen4, all)?.versionId).toBe("keen4-1.4-ega");
    expect(freeFit(["keen4-1.4-cga"], keen4, all)?.versionId).toBe("keen4-1.4-cga");
    // Any version: the setup wizard's shareware, never a demo.
    expect(freeFit([], keen4, all)?.versionId).toBe("keen4-1.4-ega");
    expect(freeFit([], keen4, [free("keen4-demo")])).toBeUndefined();
  });

  it("has none for versions that aren't free, or one already there", () => {
    expect(freeFit(["keen4-1.1-ega"], keen4, all)).toBeUndefined();
    expect(freeFit(["keen4-1.4-ega"], keen4, [free("keen4-1.4-ega", true, true)])).toBeUndefined();
    expect(freeFit([], undefined, all)).toBeUndefined();
  });
});
