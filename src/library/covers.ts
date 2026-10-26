import { useQuery, type QueryClient } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { profilesKey } from "./queries";

/** All covers. */
export const coverKeys = ["cover"] as const;
/**
 * Invalidate it when a profile's files change, e.g. a mod is replaced; remove
 * it with the profile, whose id a new one may get.
 */
export const coverKey = (profileId: string) => [...coverKeys, profileId] as const;

/**
 * Reloads the profiles after some were removed and drops the covers of the
 * ones gone: a profile added later may get the same id. Mods moved to another
 * copy of their game get another try anyway (`useCover`).
 */
export async function forgetRemovedCovers(client: QueryClient) {
  try {
    const profiles = await client.fetchQuery({
      queryKey: profilesKey,
      queryFn: () => unwrap(commands.listProfiles()),
      staleTime: 0,
    });
    const ids = new Set(profiles.map((p) => p.id));
    client.removeQueries({
      queryKey: coverKeys,
      predicate: (q) => !ids.has(String(q.queryKey[1])),
    });
  } catch {
    // Without the list, none can be trusted.
    client.removeQueries({ queryKey: coverKeys });
  }
}

/**
 * The image URL of a profile's title screen, extracted from its game files
 * on first use, and whether its graphics are scrambled; `null` keeps the
 * placeholder. A mod that gets its game gets an install, and with it another
 * try.
 */
export function useCover(profileId: string, installId: string | null) {
  return useQuery({
    queryKey: [...coverKey(profileId), installId],
    queryFn: async () => {
      const cover = await unwrap(commands.profileCover(profileId));
      if (cover === null) return null;
      // A new extraction keeps the file name, so the URL has to change.
      return { src: `${convertFileSrc(cover.path)}?v=${Date.now()}`, scrambled: cover.scrambled };
    },
    staleTime: Infinity,
    retry: false,
  });
}
