import { useQuery } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type Screenshot } from "../bindings";
import { unwrap } from "../ipc";

export const screenshotsKey = (profileId: string) => ["screenshots", profileId] as const;

export function useScreenshots(profileId: string) {
  return useQuery({
    queryKey: screenshotsKey(profileId),
    queryFn: () => unwrap(commands.listScreenshots(profileId)),
  });
}

/** An image URL for a screenshot on disk (asset protocol). */
export function screenshotSrc(shot: Screenshot): string {
  return convertFileSrc(shot.path);
}
