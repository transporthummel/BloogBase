import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, events } from "../bindings";
import { unwrap } from "../ipc";

export const indexStatusKey = ["indexStatus"] as const;
export const indexModsKey = ["indexMods"] as const;

export function useIndexStatus() {
  return useQuery({ queryKey: indexStatusKey, queryFn: commands.indexStatus });
}

export function useIndexMods() {
  return useQuery({ queryKey: indexModsKey, queryFn: () => unwrap(commands.indexMods()) });
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
