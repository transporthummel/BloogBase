import { queryOptions, useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

export const profilesKey = ["profiles"] as const;
export const installsKey = ["installs"] as const;
export const modsKey = ["mods"] as const;

/** For `fetchQuery`, outside of components. */
export const profilesQuery = queryOptions({
  queryKey: profilesKey,
  queryFn: () => unwrap(commands.listProfiles()),
});

export function useProfiles({ enabled = true }: { enabled?: boolean } = {}) {
  return useQuery({ ...profilesQuery, enabled });
}

export function useInstalls() {
  return useQuery({ queryKey: installsKey, queryFn: () => unwrap(commands.listInstalls()) });
}

export function useMods({ enabled = true }: { enabled?: boolean } = {}) {
  return useQuery({ queryKey: modsKey, queryFn: () => unwrap(commands.listMods()), enabled });
}
