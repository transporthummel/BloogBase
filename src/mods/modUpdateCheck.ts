import { useMutation, useQueryClient, type UseMutationResult } from "@tanstack/react-query";
import { commands, type Mod, type ModUpdate } from "../bindings";
import { modsKey, profilesKey } from "../library/queries";
import { unwrap } from "../ipc";

export type ModUpdateCheck = UseMutationResult<ModUpdate, Error, void>;

/**
 * Asks a KeenWiki mod's page for a newer version; `ModUpdate` shows the
 * answer. Mods from the index aren't asked: the index's update shows by
 * itself (`IndexModUpdate`), and one from KeenWiki would take them out of it.
 */
export function useModUpdateCheck(mod: Mod | undefined): ModUpdateCheck {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async (): Promise<ModUpdate> => {
      if (!mod) throw new Error("no mod to check");
      const update = await unwrap(commands.checkModUpdate(mod.id));
      // It took over what the page now says about the mod.
      if (update.refreshed.length) client.invalidateQueries({ queryKey: modsKey });
      // The library shows its status.
      if (update.refreshed.includes("status")) {
        client.invalidateQueries({ queryKey: profilesKey });
      }
      return update;
    },
  });
}

/** Whether the mod's KeenWiki page is where its updates come from. */
export function checksKeenwiki(mod: Mod | undefined): mod is Mod {
  return !!mod && !mod.indexId && !!mod.keenwikiUrl;
}
