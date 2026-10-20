import { describe, expect, it } from "vitest";
import type { Game, GameVersion } from "./bindings";
import { bestFit } from "./catalog";

const version = (id: string, extra: Partial<GameVersion> = {}): GameVersion => ({
  id,
  label: id,
  files: {},
  shareware: false,
  ...extra,
});

const keen6 = {
  id: "keen6",
  versions: [
    version("keen6-1.0"),
    version("keen6-1.0-nocp", { original: "keen6-1.0" }),
    version("keen6-1.4-ega"),
    version("keen6-1.5-ega"),
  ],
  preferredVersions: ["keen6-1.4-ega", "keen6-1.5-ega", "keen6-1.0"],
} as Game;

const copy = (id: string, versionId: string | null) => ({ id, versionId });

describe("bestFit", () => {
  it("takes the preferred version, else the copy added first", () => {
    const pick = (...c: ReturnType<typeof copy>[]) => bestFit(keen6, c)?.id;
    expect(pick(copy("a", "keen6-1.0"), copy("b", "keen6-1.5-ega"))).toBe("b");
    expect(pick(copy("a", "keen6-1.5-ega"), copy("b", "keen6-1.4-ega"))).toBe("b");
    expect(pick(copy("a", "keen6-1.4-ega"), copy("b", "keen6-1.4-ega"))).toBe("a");
  });

  it("puts releases before builds before unknown versions", () => {
    const pick = (...c: ReturnType<typeof copy>[]) => bestFit(keen6, c)?.id;
    expect(pick(copy("a", null), copy("b", "keen6-1.0-nocp"), copy("c", "keen6-1.0"))).toBe("c");
    expect(pick(copy("a", null), copy("b", "keen6-1.0-nocp"))).toBe("b");
    expect(bestFit(keen6, [])).toBeUndefined();
  });
});
