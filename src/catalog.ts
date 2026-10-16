import { useQuery } from "@tanstack/react-query";
import { commands, type Game } from "./bindings";

/** The static game catalog; it never changes while the app runs. */
export function useGames() {
  return useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
}

export function gameTitle(games: Game[] | undefined, id: string): string {
  return games?.find((g) => g.id === id)?.title ?? id;
}
