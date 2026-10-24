import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Download } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Mod, type ModUpdate as Update } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { AddModDialog } from "./AddModDialog";
import { DownloadError } from "./DownloadError";
import type { ModUpdateCheck } from "./modUpdateCheck";
import { downloaded, type Downloaded } from "./wikiDownload";

/** What a check found, and installing a newer version over the old one. */
export function ModUpdate({
  mod,
  check,
  disabled,
}: {
  mod: Mod;
  check: ModUpdateCheck;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [fetched, setFetched] = useState<Downloaded | null>(null);
  const download = useMutation({
    mutationFn: async ({ page }: Update) => {
      const newest = page.downloads[0];
      const d = await unwrap(commands.downloadKeenwikiMod(newest.url, page.gameId));
      return downloaded(page, newest, d);
    },
    onSuccess: setFetched,
  });

  const update = check.data;
  const newest = update?.page.downloads[0];
  const offered = update && (update.status === "newer" || update.status === "unknown");

  if (check.isIdle && !download.error && !fetched) return null;

  return (
    <div className="grid justify-items-start gap-2">
      {check.isPending && <Notice>{t("modUpdate.checking")}</Notice>}
      {check.error && <Notice tone="error">{errorMessage(t, check.error)}</Notice>}
      {update?.status === "upToDate" && <Notice>{t("modUpdate.upToDate")}</Notice>}
      {update?.status === "noDownload" && <Notice>{t("modUpdate.noDownload")}</Notice>}
      {offered && newest && (
        <>
          <Notice>
            {update.status === "newer"
              ? t("modUpdate.newer", {
                  version: newest.version,
                  context: newest.version ? "version" : undefined,
                })
              : t("modUpdate.unknown", { download: newest.label })}
          </Notice>
          {mod.indexId && <Notice>{t("modUpdate.leavesIndex")}</Notice>}
          <Button onClick={() => download.mutate(update)} disabled={disabled || download.isPending}>
            <Download className="size-4" aria-hidden />
            {download.isPending ? t("modUpdate.downloading") : t("modUpdate.update")}
          </Button>
        </>
      )}
      {download.error && download.variables && (
        <DownloadError error={download.error} url={download.variables.page.downloads[0].url} />
      )}
      {fetched && (
        <AddModDialog
          downloaded={fetched}
          replacing={mod.id}
          onClose={() => setFetched(null)}
          onInstalled={() => {
            setFetched(null);
            check.reset();
            queryClient.invalidateQueries({ queryKey: profilesKey });
          }}
        />
      )}
    </div>
  );
}
