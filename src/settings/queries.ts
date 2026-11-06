import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type GameSettings } from "../bindings";
import { unwrap } from "../ipc";

export const gameSettingsKey = ["gameSettings"] as const;

/** The settings for all games, and a way to change some of them. */
export function useGameSettings() {
  const queryClient = useQueryClient();
  const settings = useQuery({
    queryKey: gameSettingsKey,
    queryFn: () => unwrap(commands.gameSettings()),
  });
  const save = useMutation({
    mutationFn: (s: GameSettings) => unwrap(commands.setGameSettings(s)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: gameSettingsKey }),
  });
  const data = settings.data;
  return {
    settings: data,
    update: (change: Partial<GameSettings>) => {
      if (data) save.mutate({ ...data, ...change });
    },
    pending: save.isPending,
    error: settings.error ?? save.error,
  };
}
