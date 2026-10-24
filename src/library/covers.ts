import { useQuery } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

/** All covers. */
export const coverKeys = ["cover"] as const;
/**
 * Invalidate it when a profile's files change, e.g. a mod is replaced; remove
 * it with the profile, whose id a new one may get.
 */
export const coverKey = (profileId: string) => [...coverKeys, profileId] as const;

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
