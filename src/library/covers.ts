import { useQuery } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "../bindings";
import { unwrap } from "../ipc";

/**
 * The image URL of a profile's title screen, extracted from its game files
 * on first use; `null` keeps the placeholder.
 */
export function useCover(profileId: string) {
  return useQuery({
    queryKey: ["cover", profileId],
    queryFn: async () => {
      const path = await unwrap(commands.profileCover(profileId));
      return path === null ? null : convertFileSrc(path);
    },
    staleTime: Infinity,
    retry: false,
  });
}
