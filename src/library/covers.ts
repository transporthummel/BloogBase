import { useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { profilesQuery } from "./queries";

/** All covers. */
export const coverKeys = ["cover"] as const;
/**
 * Invalidate it when a profile's files change, e.g. a mod is replaced; remove
 * it with the profile, whose id a new one may get.
 */
export const coverKey = (profileId: string) => [...coverKeys, profileId] as const;

/**
 * Reloads the profiles after some were removed and drops the covers of the
 * ones gone, and those of mods for a copy of their game they're no longer
 * on: a profile or copy added later may get the same id, and a mod that
 * waited for its game has lost its cover meanwhile. Other moves to another
 * copy get another try anyway (`useCover`).
 */
export async function forgetRemovedCovers(client: QueryClient) {
  try {
    const profiles = await client.fetchQuery({ ...profilesQuery, staleTime: 0 });
    const installs = new Map(profiles.map((p) => [p.id, p.installId]));
    client.removeQueries({
      queryKey: coverKeys,
      predicate: (q) => {
        const [, id, installId] = q.queryKey;
        return !installs.has(String(id)) || installs.get(String(id)) !== installId;
      },
    });
  } catch {
    // Without the list, none can be trusted.
    client.removeQueries({ queryKey: coverKeys });
  }
}

type CoverData = { src: string; scrambled: boolean; retried?: boolean } | null;

async function loadCover(profileId: string, retried = false): Promise<CoverData> {
  const cover = await unwrap(commands.profileCover(profileId));
  if (cover === null) return null;
  // A new extraction keeps the file name, so the URL has to change.
  return {
    src: `${convertFileSrc(cover.path)}?v=${Date.now()}`,
    scrambled: cover.scrambled,
    retried,
  };
}

/**
 * The image URL of a profile's title screen, extracted from its game files
 * on first use, and whether its graphics are scrambled; `null` keeps the
 * placeholder. A mod that gets its game gets an install, and with it another
 * try.
 *
 * `onError` goes on the image: a cached cover whose file is gone (it has
 * happened in many ways) is asked for once more, which extracts it again;
 * if that one fails too, the placeholder stays.
 */
export function useCover(profileId: string, installId: string | null) {
  const client = useQueryClient();
  const queryKey = [...coverKey(profileId), installId];
  const query = useQuery({
    queryKey,
    queryFn: () => loadCover(profileId),
    staleTime: Infinity,
    retry: false,
  });
  const onError = (src: string) => {
    const current = client.getQueryData<CoverData>(queryKey);
    // An image replaced meanwhile, or more than one showing the same cover.
    if (current?.src !== src) return;
    if (current.retried) {
      client.setQueryData<CoverData>(queryKey, null);
    } else {
      client
        .fetchQuery({ queryKey, queryFn: () => loadCover(profileId, true), staleTime: 0 })
        .catch(() => client.setQueryData<CoverData>(queryKey, null));
    }
  };
  return { data: query.data, isPending: query.isPending, onError };
}
