import type { FreeGame, Game, IndexEntry, Install } from "../bindings";

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
 * `requiresGame`, which the app sets on standalone mods of games without
 * free shareware (`IndexEntry::needs_no_game`); those are listed and
 * installed like any other mod. */
export function needsNoGame(entry: IndexEntry): boolean {
  return !!entry.standalone && !entry.requiresGame;
}

/** A standalone mod that shares nothing with its game but the engine, known
 * by the programs its downloads list; one with all of Keen 4's files is a
 * Keen 4 mod that needs no copy of it. */
export function isSourceMod(entry: IndexEntry): boolean {
  return (
    needsNoGame(entry) && entry.downloads.some((d) => Object.keys(d.programs ?? {}).length > 0)
  );
}

/** Whether `entry` can be played with `owned`: on a copy it fits, on none
 * at all for a standalone mod, or on a free version that comes along. */
export function playable(
  entry: IndexEntry,
  owned: Owned[],
  games: Game[] = [],
  free: FreeGame[] = [],
): boolean {
  const game = games.find((g) => g.id === entry.game);
  return (
    needsNoGame(entry) ||
    fitting(entry, owned, games).length > 0 ||
    freeFit(entry.requires, game, free) !== undefined
  );
}

/** Whether `entry` is listed under `game`, a game id or {@link NO_GAME};
 * source mods are under no game, not the one they're built on, and mods
 * with all of their game's files under both. */
export function listedUnder(entry: IndexEntry, game: string): boolean {
  return game === NO_GAME ? needsNoGame(entry) : !isSourceMod(entry) && entry.game === game;
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

/**
 * The free version of `game` to install along with a mod that plays on
 * `requires` (empty: any), for a player without a copy it fits: a full
 * shareware release, the setup wizard's one first (decided 2026-11-14).
 */
export function freeFit(
  requires: string[],
  game: Game | undefined,
  free: FreeGame[],
): FreeGame | undefined {
  const fits = (f: FreeGame) => {
    const v = game?.versions.find((v) => v.id === f.versionId);
    if (!v || (v.kind ?? "release") !== "release") return false;
    return (
      requires.length === 0 ||
      requires.includes(v.id) ||
      (v.original != null && requires.includes(v.original))
    );
  };
  const options = free.filter((f) => f.gameId === game?.id && !f.installed && fits(f));
  return options.find((f) => f.setup) ?? options[0];
}
