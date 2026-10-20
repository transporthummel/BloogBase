import { useQuery } from "@tanstack/react-query";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

export const setupStatusKey = ["setupStatus"] as const;

export function useSetupStatus() {
  return useQuery({ queryKey: setupStatusKey, queryFn: () => unwrap(commands.setupStatus()) });
}
