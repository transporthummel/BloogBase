import { describe, expect, it } from "vitest";
import type {
  Game,
  GameVersion,
  Install,
  KeenwikiOrigin,
  Mod,
  ModInfo,
  RecipeOption,
} from "../bindings";
import { otherVersions, pickTarget, setUpWarnings } from "./setUpChecks";

const version = (id: string, extra: Partial<GameVersion> = {}): GameVersion => ({
  id,
  label: id,
  files: {},
  shareware: false,
  ...extra,
});

const keen4 = {
  id: "keen4",
  versions: [
    version("keen4-1.4-ega"),
    version("keen4-1.4-nocp", { original: "keen4-1.4-ega" }),
    version("keen4-1.0"),
  ],
  moddingVersion: "keen4-1.4-ega",
  preferredVersions: ["keen4-1.4-ega", "keen4-1.0"],
} as Game;

const copy = (id: string, versionId: string | null) =>
  ({ id, gameId: "keen4", versionId }) as Install;

const patch: RecipeOption = {
  recipe: { type: "patch", patcher: "CK4PATCH.EXE", patch: "MOD.PAT", args: [] },
  origin: "PLAY.BAT",
  gameVersions: [],
  versionIds: [],
  missingFiles: [],
  baseMod: null,
  madeFor: null,
};

const info = (extra: Partial<ModInfo> = {}): ModInfo => ({
  name: "Aurora",
  authors: [],
  version: null,
  released: null,
  gameId: "keen4",
  recipes: [patch],
  confident: true,
  fileCount: 3,
  indexId: null,
  ...extra,
});

const installed = (id: string, extra: Partial<Mod> = {}) =>
  ({ id, gameId: "keen4", name: id, keenwikiUrl: null, indexId: null, ...extra }) as Mod;

const page = (pageUrl: string, gameId: string | null = null) =>
  ({ pageUrl, gameId }) as KeenwikiOrigin;

describe("otherVersions", () => {
  it("finds the mod by page, by name or as picked, but not another mod of that name", () => {
    const mods = [
      installed("aurora"),
      installed("Aurora Bliss", { keenwikiUrl: "wiki/bliss" }),
      installed("picked"),
      installed("other page", { name: "Aurora", keenwikiUrl: "wiki/other" }),
      installed("other game", { name: "Aurora", gameId: "keen5" }),
    ];
    const ids = (name: string, origin: KeenwikiOrigin | null, replacing?: string) =>
      otherVersions(mods, info(), name, origin, replacing).map((m) => m.id);
    expect(ids("Aurora", null)).toEqual(["aurora", "other page"]);
    expect(ids("Aurora", page("wiki/bliss"))).toEqual(["aurora", "Aurora Bliss"]);
    expect(ids("Renamed", null, "picked")).toEqual(["aurora", "picked", "other page"]);
  });
});

describe("pickTarget", () => {
  const copies = [copy("a", "keen4-1.0"), copy("b", "keen4-1.4-ega"), copy("c", null)];
  it("takes the picked copy, else the replaced mod's, else the best fit", () => {
    expect(pickTarget(copies, "c", "a", keen4)?.id).toBe("c");
    expect(pickTarget(copies, null, "a", keen4)?.id).toBe("a");
    expect(pickTarget(copies, "gone", undefined, keen4)?.id).toBe("b");
    expect(pickTarget([], null, null, keen4)).toBeUndefined();
  });
});

describe("setUpWarnings", () => {
  const warnings = (target: Install | undefined, extra: Partial<RecipeOption> = {}) =>
    setUpWarnings({
      info: info(),
      option: { ...patch, ...extra },
      origin: null,
      game: keen4,
      candidates: target ? [target] : [],
      target,
    });

  it("assumes the modding version for patches that name none", () => {
    expect(warnings(copy("a", "keen4-1.4-ega")).assumedInstead).toBeUndefined();
    // A build of it counts as it, but patching it may not work.
    const nocp = warnings(copy("a", "keen4-1.4-nocp"));
    expect([nocp.assumedInstead, nocp.modifiedGame]).toEqual([undefined, true]);
    expect(warnings(copy("a", "keen4-1.0")).assumedInstead?.id).toBe("keen4-1.4-ega");
    // Unknown versions may well be the right one.
    expect(warnings(copy("a", null)).assumedInstead).toBeUndefined();
    // A patch naming its versions plays on those only.
    const named = { gameVersions: ["1.0"], versionIds: ["keen4-1.0"] };
    expect(warnings(copy("a", "keen4-1.0"), named).assumedInstead).toBeUndefined();
    expect(warnings(copy("a", null), named).versionUnchecked).toBe(true);
  });

  it("tells a missing game only once the library is known", () => {
    expect(warnings(undefined).missingGame).toBe(true);
    const loading = setUpWarnings({
      info: info(),
      option: patch,
      origin: null,
      game: keen4,
      candidates: undefined,
      target: undefined,
    });
    expect(loading.missingGame).toBe(false);
  });

  it("tells what a mod that doesn't run on its own needs", () => {
    const base = { name: "Lego Keen", modId: "lego", indexId: null };
    expect(warnings(undefined, { missingFiles: ["LEGO.CK4"], baseMod: base })).toMatchObject({
      incomplete: true,
      levelsFor: false,
    });
    expect(warnings(undefined, { baseMod: base })).toMatchObject({
      incomplete: false,
      levelsFor: true,
      unknownBase: null,
    });
    expect(warnings(undefined, { madeFor: "Unknown Mod" }).unknownBase).toBe("Unknown Mod");
  });

  it("tells when the page says another game than the files", () => {
    const other = (gameId: string) =>
      setUpWarnings({
        info: info(),
        option: patch,
        origin: page("wiki/x", gameId),
        game: keen4,
        candidates: [],
        target: undefined,
      }).otherGame;
    expect(other("keen5")).toBe(true);
    expect(other("keen4")).toBe(false);
  });
});
