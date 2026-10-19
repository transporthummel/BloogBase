import { useQuery } from "@tanstack/react-query";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type Screenshot } from "../bindings";
import { unwrap } from "../ipc";

export const screenshotsKey = (profileId: string) => ["screenshots", profileId] as const;
export const videosKey = (profileId: string) => ["videos", profileId] as const;

export function useScreenshots(profileId: string) {
  return useQuery({
    queryKey: screenshotsKey(profileId),
    queryFn: () => unwrap(commands.listScreenshots(profileId)),
  });
}

export function useVideos(profileId: string) {
  return useQuery({
    queryKey: videosKey(profileId),
    queryFn: () => unwrap(commands.listVideos(profileId)),
  });
}

/** An image URL for a screenshot on disk (asset protocol). */
export function screenshotSrc(shot: Screenshot): string {
  return convertFileSrc(shot.path);
}

/** "0:17", "12:05", "1:02:03". */
export function formatDuration(seconds: number): string {
  const s = String(seconds % 60).padStart(2, "0");
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}:${s}`;
  return `${Math.floor(minutes / 60)}:${String(minutes % 60).padStart(2, "0")}:${s}`;
}
