import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events } from "../bindings";
import { unwrap } from "../ipc";

export const indexStatusKey = ["indexStatus"] as const;
export const indexModsKey = ["indexMods"] as const;
export const freeGamesKey = ["freeGames"] as const;

export function useIndexStatus() {
  return useQuery({ queryKey: indexStatusKey, queryFn: commands.indexStatus });
}

export function useIndexMods() {
  return useQuery({ queryKey: indexModsKey, queryFn: () => unwrap(commands.indexMods()) });
}

export type Installed = "current" | "outdated" | null;

/** Whether the index entry `id` is installed already, so there's nothing to cancel. */
export function useIndexInstalled(id: string | null): Installed {
  const mod = useIndexMods().data?.find((m) => m.entry.id === id && m.modId);
  if (!id || !mod) return null;
  return mod.updateAvailable ? "outdated" : "current";
}

/** Shareware episodes, demos and betas BloogBase can download. */
export function useFreeGames() {
  return useQuery({ queryKey: freeGamesKey, queryFn: () => unwrap(commands.freeGames()) });
}

/** Checks for a new index right away. */
export function useRefreshIndex() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => commands.refreshIndex(true),
    onSuccess: (status) => {
      client.setQueryData(indexStatusKey, status);
      client.invalidateQueries({ queryKey: indexModsKey });
    },
  });
}

/** Follows the backend's background checks of the index. */
export function useIndexEvents() {
  const client = useQueryClient();
  useEffect(() => {
    const unlisten = events.indexUpdated.listen(({ payload: status }) => {
      client.setQueryData(indexStatusKey, status);
      client.invalidateQueries({ queryKey: indexModsKey });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [client]);
}
