import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { commands } from "../bindings";

export const updateKey = ["appUpdate"] as const;

/**
 * A newer BloogBase release, if there is one. Release builds check once on
 * start; `enabled: false` waits for a manual check.
 */
export function useAppUpdate({ enabled }: { enabled: boolean }) {
  return useQuery<Update | null>({
    queryKey: updateKey,
    queryFn: async () => (await check()) ?? null,
    enabled,
    staleTime: Infinity,
    retry: false,
  });
}

/** Checks again right away. */
export function useCheckForUpdate() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async () => (await check()) ?? null,
    onSuccess: (update) => client.setQueryData(updateKey, update),
  });
}

/** Downloads and installs the update, then restarts into it. */
export function useInstallUpdate() {
  return useMutation({
    mutationFn: async ({
      update,
      onProgress,
    }: {
      update: Update;
      onProgress: (fraction: number | null) => void;
    }) => {
      let total: number | undefined;
      let done = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength;
        if (event.event === "Progress") {
          done += event.data.chunkLength;
          onProgress(total ? done / total : null);
        }
      });
      await commands.restartApp();
    },
  });
}
