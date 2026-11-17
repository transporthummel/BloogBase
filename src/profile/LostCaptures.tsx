import { useQueries } from "@tanstack/react-query";
import { Camera } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { screenshotsKey, videosKey } from "./captures";

/**
 * Says how many screenshots and videos go with the profiles a remove dialog
 * deletes: unlike saves, no backup keeps them. Nothing while there are none.
 */
export function LostCaptures({ profileIds }: { profileIds: string[] }) {
  const { t } = useTranslation();
  const count = (results: { data?: unknown[] }[]) =>
    results.reduce((sum, r) => sum + (r.data?.length ?? 0), 0);
  const screenshots = useQueries({
    queries: profileIds.map((id) => ({
      queryKey: screenshotsKey(id),
      queryFn: () => unwrap(commands.listScreenshots(id)),
    })),
    combine: count,
  });
  const videos = useQueries({
    queries: profileIds.map((id) => ({
      queryKey: videosKey(id),
      queryFn: () => unwrap(commands.listVideos(id)),
    })),
    combine: count,
  });
  if (screenshots === 0 && videos === 0) return null;
  const parts = [
    screenshots > 0 && t("removeCaptures.screenshots", { count: screenshots }),
    videos > 0 && t("removeCaptures.videos", { count: videos }),
  ].filter((p): p is string => !!p);
  const what =
    parts.length === 2 ? t("removeCaptures.and", { a: parts[0], b: parts[1] }) : parts[0];
  return (
    <p className="flex items-start gap-2 text-sm text-muted">
      <Camera className="mt-0.5 size-4 shrink-0" aria-hidden />
      <span>{t("removeCaptures.text", { what })}</span>
    </p>
  );
}
