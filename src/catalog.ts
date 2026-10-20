import { useQuery } from "@tanstack/react-query";
import { commands, type Game, type GameVersion } from "./bindings";

/** The static game catalog; it never changes while the app runs. */
export function useGames() {
  return useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
}

export function gameTitle(games: Game[] | undefined, id: string): string {
  return games?.find((g) => g.id === id)?.title ?? id;
}

/** A third-party build whose code differs from its original release. */
export function isModified(version: GameVersion | undefined): boolean {
  return version?.original != null && !version.repackaged;
}
