import { useQuery } from "@tanstack/react-query";
import { commands, type Game, type GameVersion, type Install } from "./bindings";

/** The static game catalog; it never changes while the app runs. */
export function useGames() {
  return useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
}

export function gameTitle(games: Game[] | undefined, id: string): string {
  return games?.find((g) => g.id === id)?.title ?? id;
}

/** "Keen 4", for where a version label follows. */
export function shortGameTitle(games: Game[] | undefined, id: string): string {
  return games?.find((g) => g.id === id)?.shortTitle ?? id;
}

/** A third-party build whose code differs from its original release. */
export function isModified(version: GameVersion | undefined): boolean {
  return version?.original != null && !version.repackaged;
}

/**
 * The install a mod goes on by default, out of those it fits; mirrors
 * `fit_rank` in mods.rs: releases, then third-party builds, then unknown
 * versions, each by the game's preferred versions, then the copy added first.
 */
export function bestFit<T extends Pick<Install, "versionId">>(
  game: Game | undefined,
  candidates: T[],
): T | undefined {
  const preferred = game?.preferredVersions ?? [];
  const rank = (i: T): [number, number] => {
    if (i.versionId === null) return [2, 0];
    const version = game?.versions.find((v) => v.id === i.versionId);
    const place = preferred.findIndex((p) => p === i.versionId || p === version?.original);
    return [isModified(version) ? 1 : 0, place === -1 ? preferred.length : place];
  };
  let best: T | undefined;
  let bestRank: [number, number] = [Infinity, Infinity];
  for (const i of candidates) {
    const r = rank(i);
    if (r[0] < bestRank[0] || (r[0] === bestRank[0] && r[1] < bestRank[1])) {
      best = i;
      bestRank = r;
    }
  }
  return best;
}
