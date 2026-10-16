import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands, type RuntimeStatus } from "../bindings";
import { unwrap } from "../ipc";

export const runtimeKey = ["runtime"] as const;

export function useRuntimeStatus() {
  return useQuery({ queryKey: runtimeKey, queryFn: () => unwrap(commands.runtimeStatus()) });
}

/** Updates the cached status after a command that returns a new one. */
export function useRuntimeMutation<A>(fn: (arg: A) => Promise<RuntimeStatus>) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: fn,
    onSuccess: (status) => queryClient.setQueryData(runtimeKey, status),
  });
}
