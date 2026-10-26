import { expect, test } from "vitest";
import type { IndexMod } from "../bindings";
import type { Release } from "./changelog";
import { compareVersions, modUpdateKey, modUpdates, newIndexMods, releasesSince } from "./news";

const release = (version: string): Release => ({ version, en: [], de: [] });

const mod = (id: string, modId: string | null, updateAvailable = false) =>
  ({
    entry: { id, downloads: [{ url: `https://example.org/${id}-2.zip` }] },
    modId,
    updateAvailable,
  }) as IndexMod;

test("compares versions by number", () => {
  expect(compareVersions("1.10.0", "1.9.2")).toBeGreaterThan(0);
  expect(compareVersions("1.0", "1.0.0")).toBe(0);
  expect(compareVersions("0.2.0-beta", "0.2.0")).toBe(0);
  expect(compareVersions("0.1.9", "0.2.0")).toBeLessThan(0);
});

test("shows the releases since the last start, none on the first", () => {
  const log = ["0.4.0", "0.3.0", "0.2.0", "0.1.0"].map(release);
  expect(releasesSince(log, "0.1.0", "0.3.0").map((r) => r.version)).toEqual(["0.3.0", "0.2.0"]);
  expect(releasesSince(log, "0.3.0", "0.3.0")).toEqual([]);
  expect(releasesSince(log, null, "0.3.0")).toEqual([]);
});

test("new mods leave out seen and installed ones", () => {
  const mods = [mod("a", null), mod("b", null), mod("c", "c")];
  expect(newIndexMods(mods, ["a"]).map((m) => m.entry.id)).toEqual(["b"]);
});

test("offers each update of an installed mod once", () => {
  const mods = [mod("a", "a", true), mod("b", "b", true), mod("c", "c"), mod("d", null, true)];
  expect(modUpdates(mods, [modUpdateKey(mods[0])]).map((m) => m.entry.id)).toEqual(["b"]);
});
