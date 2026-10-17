import type { Game, Profile } from "../bindings";

export const librarySorts = ["recent", "name", "added", "game", "type"] as const;
export type LibrarySort = (typeof librarySorts)[number];

/** Profiles under one heading; `key` is a game id, "games" or "mods", or "all". */
export type Group = { key: string; profiles: Profile[] };

/**
 * The library in the chosen order. `profiles` come most recently played
 * first; grouped sorts keep base games before mods and the catalog's game
 * order.
 */
export function arrange(
  profiles: Profile[],
  sort: LibrarySort,
  games: Game[],
  language: string,
): Group[] {
  const collator = new Intl.Collator(language, { numeric: true, sensitivity: "base" });
  const byName = (a: Profile, b: Profile) => collator.compare(a.label, b.label);
  const gameIndex = (p: Profile) => {
    const i = games.findIndex((g) => g.id === p.gameId);
    return i === -1 ? games.length : i;
  };
  // Base games in catalog order, then mods by name.
  const gamesFirst = (a: Profile, b: Profile) =>
    Number(a.modId !== null) - Number(b.modId !== null) ||
    (a.modId === null ? gameIndex(a) - gameIndex(b) : 0) ||
    byName(a, b);

  switch (sort) {
    case "recent":
      return [{ key: "all", profiles }];
    case "name":
      return [{ key: "all", profiles: [...profiles].sort(byName) }];
    case "added":
      return [
        {
          key: "all",
          profiles: [...profiles].sort((a, b) => b.addedAt - a.addedAt || byName(a, b)),
        },
      ];
    case "game": {
      const sorted = [...profiles].sort((a, b) => gameIndex(a) - gameIndex(b) || gamesFirst(a, b));
      const groups: Group[] = [];
      for (const p of sorted) {
        const last = groups[groups.length - 1];
        if (last?.key === p.gameId) last.profiles.push(p);
        else groups.push({ key: p.gameId, profiles: [p] });
      }
      return groups;
    }
    case "type": {
      const sorted = [...profiles].sort(gamesFirst);
      return [
        { key: "games", profiles: sorted.filter((p) => p.modId === null) },
        { key: "mods", profiles: sorted.filter((p) => p.modId !== null) },
      ].filter((g) => g.profiles.length > 0);
    }
  }
}
