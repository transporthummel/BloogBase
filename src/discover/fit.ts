import type { Game, IndexEntry, Install } from "../bindings";

/** What matters about a game in the library, or one that is on its way. */
export type Owned = Pick<Install, "gameId" | "versionId">;

/**
 * Whether `entry` can be played on `owned`; mirrors `fits` in
 * mods.rs. Unknown versions get the benefit of the doubt, third-party
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

export function fitting<T extends Owned>(entry: IndexEntry, owned: T[], games: Game[] = []): T[] {
  const game = games.find((g) => g.id === entry.game);
  return owned.filter((o) => fitsEntry(entry, o, game));
}

/** Labels of the required versions, e.g. `v1.4 EGA`. */
export function requiredLabels(entry: IndexEntry, games: Game[] = []): string[] {
  const game = games.find((g) => g.id === entry.game);
  return entry.requires.map((id) => game?.versions.find((v) => v.id === id)?.label ?? id);
}

const MONTH = 30 * 24 * 60 * 60 * 1000;
const day = (date: string) => Date.parse(`${date}T00:00:00Z`);

/**
 * Added to the index in the last month, but not in the index's own first
 * month: the list was filled then, and every entry would say so.
 */
export function isNew(entry: IndexEntry, all: IndexEntry[], now = Date.now()): boolean {
  const added = day(entry.added);
  const first = Math.min(...all.map((e) => day(e.added)).filter((d) => !Number.isNaN(d)));
  return !Number.isNaN(added) && now - added < MONTH && added - first >= MONTH;
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
