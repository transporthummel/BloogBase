import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type GameSettings } from "../bindings";
import { unwrap } from "../ipc";

export const gameSettingsKey = ["gameSettings"] as const;
const saveKey = ["setGameSettings"] as const;

/** The settings for all games, and a way to change some of them. */
export function useGameSettings() {
  const queryClient = useQueryClient();
  const settings = useQuery({
    queryKey: gameSettingsKey,
    queryFn: () => unwrap(commands.gameSettings()),
  });
  const save = useMutation({
    mutationKey: saveKey,
    // One at a time, in order: each holds all settings.
    scope: { id: "gameSettings" },
    mutationFn: (s: GameSettings) => unwrap(commands.setGameSettings(s)),
    onSettled: () => {
      // A refetch before the last save would bring back an older state.
      if (queryClient.isMutating({ mutationKey: saveKey }) <= 1) {
        return queryClient.invalidateQueries({ queryKey: gameSettingsKey });
      }
    },
  });
  return {
    settings: settings.data,
    // On top of the latest change, also one still being saved or made elsewhere.
    update: (change: Partial<GameSettings>) => {
      const current = queryClient.getQueryData<GameSettings>(gameSettingsKey);
      if (!current) return;
      const next = { ...current, ...change };
      queryClient.setQueryData(gameSettingsKey, next);
      save.mutate(next);
    },
    pending: save.isPending,
    error: settings.error ?? save.error,
  };
}
