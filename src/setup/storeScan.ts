import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { commands, type StoreInstall } from "../bindings";
import { unwrap } from "../ipc";
import { installsKey, profilesKey } from "../library/queries";
import { setupStatusKey } from "./queries";

const scanKey = ["storeScan"] as const;

export const key = (install: StoreInstall, gameId: string) => `${install.path}\n${gameId}`;

/**
 * Commander Keen games installed from Steam or GOG, all not yet imported
 * ones picked until the player unticks some, and adding the picked ones.
 * Shared by the setup wizard and the library's "Find my games".
 */
/** `onImported` gets how many games were added. */
export function useStoreScan(onImported: (count: number) => void) {
  const client = useQueryClient();
  const scan = useQuery({ queryKey: scanKey, queryFn: () => unwrap(commands.scanStores()) });
  const [picked, setPicked] = useState<Set<string> | null>(null);

  const importable = (scan.data ?? []).flatMap((i) =>
    i.games.filter((g) => !g.imported).map((g) => key(i, g.gameId)),
  );
  const selected = picked ?? new Set(importable);
  const count = importable.filter((k) => selected.has(k)).length;

  const refresh = () =>
    Promise.all([
      client.invalidateQueries({ queryKey: profilesKey }),
      client.invalidateQueries({ queryKey: installsKey }),
      client.invalidateQueries({ queryKey: setupStatusKey }),
      client.invalidateQueries({ queryKey: scanKey }),
    ]);

  const importStores = useMutation({
    mutationFn: async () => {
      let added = 0;
      for (const install of scan.data ?? []) {
        const ids = install.games
          .filter((g) => !g.imported && selected.has(key(install, g.gameId)))
          .map((g) => g.gameId);
        if (ids.length > 0) {
          await unwrap(commands.importStoreGames(install.store, install.path, ids));
          added += ids.length;
        }
      }
      return added;
    },
    onSuccess: async (added) => {
      await refresh();
      onImported(added);
    },
  });

  function toggle(k: string) {
    const set = new Set(selected);
    if (set.has(k)) set.delete(k);
    else set.add(k);
    setPicked(set);
  }

  return { scan, selected, toggle, count, importStores, refresh };
}
