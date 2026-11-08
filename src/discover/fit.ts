import type { Game, IndexEntry, Install } from "../bindings";

/** What matters about a game in the library, or one that is on its way. */
export type Owned = Pick<Install, "gameId" | "versionId">;

/**
 * Whether `entry` can be played on `owned`; mirrors `fits` in
 * mods/fit.rs. Unknown versions get the benefit of the doubt, third-party
 * builds count as their original.
 */
export function fitsEntry(entry: IndexEntry, owned: Owned, game: Game | undefined): boolean {
  if (owned.gameId !== entry.game) return false;
  if (entry.requires.length === 0 || owned.versionId === null) return true;
  const original = game?.versions.find((v) => v.id === owned.versionId)?.original;
  return (
    entry.requires.includes(owned.versionId) ||
    (original != null && entry.requires.includes(original))
  );
}

/** Discover's game filter for mods that need no game. */
export const NO_GAME = "none";

/** Whether `entry` plays without a copy of its game: standalone, and not
 * `requiresGame` (source mods that carry the game's own graphics or sounds
 * are listed and installed like any other mod). */
export function needsNoGame(entry: IndexEntry): boolean {
  return !!entry.standalone && !entry.requiresGame;
}

/** Whether `entry` can be played with `owned`: on a copy it fits, or on none
 * at all for a standalone mod. */
export function playable(entry: IndexEntry, owned: Owned[], games: Game[] = []): boolean {
  return needsNoGame(entry) || fitting(entry, owned, games).length > 0;
}

/** Whether `entry` is listed under `game`, a game id or {@link NO_GAME};
 * standalone mods are under no game, not the one they're built on. */
export function listedUnder(entry: IndexEntry, game: string): boolean {
  return game === NO_GAME ? needsNoGame(entry) : !needsNoGame(entry) && entry.game === game;
}

export function fitting<T extends Owned>(entry: IndexEntry, owned: T[], games: Game[] = []): T[] {
  const game = games.find((g) => g.id === entry.game);
  return owned.filter((o) => fitsEntry(entry, o, game));
}

/** Labels of the required versions, e.g. `v1.4 EGA`. */
export function requiredLabels(entry: IndexEntry, games: Game[] = []): string[] {
  const game = games.find((g) => g.id === entry.game);
  return entry.requires.map((id) => game?.versions.find((v) => v.id === id)?.label ?? id);
}

export const SORTS = ["featured", "newest", "name"] as const;
export type Sort = (typeof SORTS)[number];
export const isSort = (v: unknown): v is Sort => SORTS.includes(v as Sort);

export function compare(sort: Sort) {
  const byName = (a: IndexEntry, b: IndexEntry) => a.name.localeCompare(b.name);
  const byAdded = (a: IndexEntry, b: IndexEntry) =>
    b.added.localeCompare(a.added) || (b.released ?? "").localeCompare(a.released ?? "");
  return (a: IndexEntry, b: IndexEntry) => {
    if (sort === "name") return byName(a, b);
    if (sort === "featured" && !!a.featured !== !!b.featured) return a.featured ? -1 : 1;
    return byAdded(a, b) || byName(a, b);
  };
}
