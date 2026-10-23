import { useMutation, type UseMutationResult } from "@tanstack/react-query";
import { commands, type ModUpdate } from "../bindings";
import { unwrap } from "../ipc";

export type ModUpdateCheck = UseMutationResult<ModUpdate, Error, void>;

/** Asks a mod's KeenWiki page for a newer version; `ModUpdate` shows the answer. */
export function useModUpdateCheck(modId: string): ModUpdateCheck {
  return useMutation({
    mutationFn: () => unwrap(commands.checkModUpdate(modId)),
  });
}
