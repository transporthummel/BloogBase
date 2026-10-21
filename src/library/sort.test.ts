import { expect, test } from "vitest";
import type { Game, Profile } from "../bindings";
import { arrange } from "./sort";

const game = (id: string): Game => ({
  id,
  title: id,
  shortTitle: id,
  engine: "galaxy",
  executables: [],
  requiredFiles: [],
  saves: [],
  stateFiles: [],
  versions: [],
});
const games = ["keen1", "keen4", "kdreams"].map(game);

const profile = (id: string, gameId: string, label: string, addedAt: number, mod = false) =>
  ({ id, gameId, label, addedAt, modId: mod ? id : null }) as Profile;

// Most recently played first, as the backend lists them.
const profiles = [
  profile("aurora", "keen4", "Aurora Bliss", 30, true),
  profile("kdreams-1", "kdreams", "Keen Dreams", 10),
  profile("keen4-1", "keen4", "Keen 4", 20),
  profile("fire", "keen1", "fireblaze", 40, true),
  profile("keen1-1", "keen1", "Keen 1", 5),
  profile("keen10", "keen4", "Keen 10", 1, true),
];

const ids = (sort: Parameters<typeof arrange>[1]) =>
  arrange(profiles, sort, games, "en").map((g) => [g.key, g.profiles.map((p) => p.id)]);

test("flat sorts keep one list", () => {
  expect(ids("recent")).toEqual([["all", profiles.map((p) => p.id)]]);
  // Case-insensitive, numbers by value.
  expect(ids("name")).toEqual([
    ["all", ["aurora", "fire", "keen1-1", "keen4-1", "keen10", "kdreams-1"]],
  ]);
  expect(ids("added")).toEqual([
    ["all", ["fire", "aurora", "keen4-1", "kdreams-1", "keen1-1", "keen10"]],
  ]);
});

test("grouped sorts put base games first in catalog order", () => {
  expect(ids("game")).toEqual([
    ["keen1", ["keen1-1", "fire"]],
    ["keen4", ["keen4-1", "aurora", "keen10"]],
    ["kdreams", ["kdreams-1"]],
  ]);
  expect(ids("type")).toEqual([
    ["games", ["keen1-1", "keen4-1", "kdreams-1"]],
    ["mods", ["aurora", "fire", "keen10"]],
  ]);
  expect(arrange([profiles[1]], "type", games, "en").map((g) => g.key)).toEqual(["games"]);
});

test("copies of one game keep their order whichever was played last", () => {
  const keen4 = {
    ...game("keen4"),
    versions: ["keen4-1.0-ega", "keen4-1.4-ega"].map((id) => ({
      id,
      label: id,
      files: {},
      shareware: true,
    })),
  };
  const copy = (id: string, versionId: string | null, addedAt: number) =>
    ({ ...profile(id, "keen4", "Keen 4", addedAt), versionId }) as Profile;
  const copies = [
    copy("unknown", null, 1),
    copy("v14", "keen4-1.4-ega", 2),
    copy("v10-later", "keen4-1.0-ega", 4),
    copy("v10", "keen4-1.0-ega", 3),
  ];
  const order = (list: Profile[]) =>
    arrange(list, "name", [keen4], "en")[0].profiles.map((p) => p.id);
  expect(order(copies)).toEqual(["v10", "v10-later", "v14", "unknown"]);
  expect(order([...copies].reverse())).toEqual(order(copies));
});
