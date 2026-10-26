import type { IndexMod } from "../bindings";
import type { Release } from "./changelog";

/** Compares `1.10.0` with `1.9.2` by number; anything after a `-` is ignored. */
export function compareVersions(a: string, b: string): number {
  const parts = (v: string) =>
    v
      .split("-")[0]
      .split(".")
      .map((p) => Number(p) || 0);
  const pa = parts(a);
  const pb = parts(b);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/** The releases after `last` up to `current`; none on the first start (`last` null). */
export function releasesSince(
  changelog: Release[],
  last: string | null,
  current: string,
): Release[] {
  if (last === null) return [];
  return changelog.filter(
    (r) => compareVersions(r.version, last) > 0 && compareVersions(r.version, current) <= 0,
  );
}

/** Entries that joined the mod list since `seen`, leaving out ones installed already. */
export function newIndexMods(mods: IndexMod[], seen: string[]): IndexMod[] {
  return mods.filter((m) => !m.modId && !seen.includes(m.entry.id));
}

/** Identifies one update of an installed index mod: the entry and its new download. */
export function modUpdateKey(mod: IndexMod): string {
  return `${mod.entry.id}@${mod.entry.downloads[0]?.url ?? ""}`;
}

/** Installed index mods with an update not offered before. */
export function modUpdates(mods: IndexMod[], offered: string[]): IndexMod[] {
  return mods.filter((m) => m.modId && m.updateAvailable && !offered.includes(modUpdateKey(m)));
}
