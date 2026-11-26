import { type IndexSeries } from "../bindings";

/** Three episodes of index mods and a level pack. */
export const toast: IndexSeries = {
  id: "toast",
  name: "The Universe is Toast!",
  description: "Keen against the Shikadi.",
  translations: { de: { name: "Das Universum ist Toast!" } },
  episodes: [
    { mod: "keys", title: "The Keys of Krodacia", number: "1" },
    { mod: "desert", title: "Dead in the Desert", number: "2" },
    { mod: "brains", number: "3" },
  ],
  bonus: [{ mod: "desert-pack" }],
};

/** The original games as a series. */
export const galaxy: IndexSeries = {
  id: "keen-4-6",
  name: "Commander Keen 4–6",
  episodes: [
    { game: "keen4", title: "Secret of the Oracle", number: "1" },
    { game: "keen5", title: "The Armageddon Machine", number: "2" },
    { game: "keen6", number: "3" },
  ],
  bonus: [],
};
