import { expect, test } from "vitest";
import { type IndexMod } from "../bindings";
import { galaxy, toast } from "../test/series";
import {
  episodeTitle,
  profileKey,
  seriesName,
  seriesOf,
  seriesPlaces,
  seriesSearchText,
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
