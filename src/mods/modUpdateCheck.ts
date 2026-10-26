import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import { commands, type IndexMod, type Mod, type ModUpdate } from "../bindings";
import { indexModsKey, indexStatusKey } from "../discover/queries";
import { unwrap } from "../ipc";

/**
 * What a check found: the index's entry (null when it left the index), or what
 * the KeenWiki page offers.
 */
export type UpdateFound =
  { source: "index"; mod: IndexMod | null } | { source: "keenwiki"; update: ModUpdate };

export type ModUpdateCheck = UseMutationResult<UpdateFound, Error, void>;

/**
 * Asks for a newer version of a mod; `ModUpdate` shows the answer. A mod from
 * the index asks a fresh index first and its KeenWiki page only when the
 * index has nothing newer: updating from there takes it out of the index.
 */
export function useModUpdateCheck(mod: Mod | undefined): ModUpdateCheck {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<UpdateFound> => {
      if (!mod) throw new Error("no mod to check");
      if (mod.indexId) {
        const status = await commands.refreshIndex(true);
        client.setQueryData(indexStatusKey, status);
        const mods = await unwrap(commands.indexMods());
        client.setQueryData(indexModsKey, mods);
        const found = mods.find((m) => m.entry.id === mod.indexId) ?? null;
        if (found?.updateAvailable || !mod.keenwikiUrl) {
          // An old index can't say there's nothing newer.
          if (status.error && !found?.updateAvailable) throw status.error;
          return { source: "index", mod: found };
        }
      }
      return { source: "keenwiki", update: await unwrap(commands.checkModUpdate(mod.id)) };
    },
  });
}
