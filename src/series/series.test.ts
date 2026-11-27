import { expect, test } from "vitest";
import { type IndexMod } from "../bindings";
import { galaxy, toast } from "../test/series";
import { entry, game, indexMod } from "../test/seriesData";
import {
  episodeTitle,
  profileKey,
  seriesName,
  seriesOf,
  seriesPlaces,
  seriesSearchText,
  seriesYears,
  type PartState,
} from "./series";

const places = seriesPlaces([toast, galaxy]);

test("a part knows its number and its neighbours", () => {
  const desert = seriesOf(places, { indexId: "desert" })!;
  expect(desert.series.id).toBe("toast");
  expect(desert.number).toBe("2");
  expect(desert.index).toBe(1);
  expect(desert.prev?.mod).toBe("keys");
  expect(desert.next?.mod).toBe("brains");

  const keys = seriesOf(places, { indexId: "keys" })!;
  expect(keys.prev).toBeUndefined();
});

test("bonus parts come after the episodes, without a number", () => {
  const pack = seriesOf(places, { indexId: "desert-pack" })!;
  expect(pack.number).toBe("bonus");
  expect(pack.index).toBe(3);
  expect(pack.prev?.mod).toBe("brains");
  expect(pack.next).toBeUndefined();
});

test("games are parts too, mods and games apart", () => {
  expect(seriesOf(places, { gameId: "keen5" })?.number).toBe("2");
  expect(seriesOf(places, { indexId: "keen5" })).toBeUndefined();
  expect(seriesOf(places, { indexId: "nothing" })).toBeUndefined();
});

test("a profile finds its part: a mod through its index entry, a game by its id", () => {
  const indexMods = [{ entry: { id: "desert" }, modId: "desert-2" }] as IndexMod[];
  expect(profileKey({ modId: "desert-2", gameId: "keen4" }, indexMods)).toEqual({
    indexId: "desert",
  });
  expect(profileKey({ modId: null, gameId: "keen4" }, indexMods)).toEqual({ gameId: "keen4" });
  // A mod not from the index is in no series.
  expect(profileKey({ modId: "own", gameId: "keen4" }, indexMods)).toBeNull();
});

test("titles and names", () => {
  expect(episodeTitle(toast.episodes[0], "The Universe is Toast!: Keys")).toBe(
    "The Keys of Krodacia",
  );
  expect(episodeTitle(toast.episodes[2], "Battle of the Brains")).toBe("Battle of the Brains");
  expect(seriesName(toast, "de")).toBe("Das Universum ist Toast!");
  expect(seriesName(toast, "en")).toBe("The Universe is Toast!");
  expect(seriesSearchText(seriesOf(places, { indexId: "keys" }), "de")).toEqual([
    "the universe is toast!",
    "das universum ist toast!",
  ]);
});

test("a series' years run from its first part's release to its last's", () => {
  const part = (more: Partial<PartState>): PartState => ({
    key: "k",
    part: {},
    number: "1",
    title: "t",
    profileId: null,
    played: false,
    ...more,
  });
  const mod = (released: string | null) =>
    part({ indexMod: indexMod(entry("m", "M", { released })) });
  const keen4 = game("keen4", "Commander Keen 4");
  keen4.versions = [
    { ...keen4.versions[0], released: "1991-12-15" },
    { ...keen4.versions[0], released: "1990-05", kind: "beta" },
    { ...keen4.versions[0], released: "1996-12-13" },
  ];

  expect(seriesYears([mod("2012-01-05"), mod("2006-03"), mod(null)], [])).toBe("2006–2012");
  expect(seriesYears([mod("2009"), mod("2009-11")], [])).toBe("2009");
  // A game from its first full release; a beta doesn't count.
  expect(seriesYears([part({ part: { game: "keen4" } })], [keen4])).toBe("1991");
  expect(seriesYears([mod(null)], [])).toBeNull();
});
