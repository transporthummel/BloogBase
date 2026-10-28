import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Download, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type IndexMod, type Mod, type ModUpdate as Update } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { AddModDialog } from "./AddModDialog";
import { DownloadError } from "./DownloadError";
import { DownloadProgress } from "./DownloadProgress";
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

  const found = check.data;
  const update = found?.source === "keenwiki" ? found.update : undefined;
  const newest = update?.page.downloads[0];
  const offered = update && (update.status === "newer" || update.status === "unknown");

  if (check.isIdle && !download.error && !fetched) return null;
  const dismiss = () => {
    check.reset();
    download.reset();
  };

  return (
    <div className="grid justify-items-start gap-2">
      {check.isPending && <Notice>{t("modUpdate.checking")}</Notice>}
      {check.error && <Notice tone="error">{errorMessage(t, check.error)}</Notice>}
      {found?.source === "index" &&
        (found.mod ? (
          <IndexUpdate mod={found.mod} disabled={disabled} onDismiss={dismiss} />
        ) : (
          <Notice>{t("modUpdate.notInIndex")}</Notice>
        ))}
      {update && update.refreshed.length > 0 && (
        <Notice>
          {t("modUpdate.refreshed", {
            details: update.refreshed.map((d) => t(`modUpdate.detail.${d}`)).join(", "),
          })}
        </Notice>
      )}
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
          <div className="flex flex-wrap gap-2">
            <Button
              onClick={() => download.mutate(update)}
              disabled={disabled || download.isPending}
            >
              <Download className="size-4" aria-hidden />
              {download.isPending ? t("modUpdate.downloading") : t("modUpdate.update")}
            </Button>
            <Button variant="secondary" onClick={dismiss} disabled={download.isPending}>
              {t("modUpdate.dismiss")}
            </Button>
          </div>
          {download.isPending && <DownloadProgress url={newest.url} />}
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

/** The index's newer version, installed over the old one like from Discover. */
function IndexUpdate({
  mod,
  disabled,
  onDismiss,
}: {
  mod: IndexMod;
  disabled: boolean;
  onDismiss: () => void;
}) {
  const { t } = useTranslation();
  const { entry } = mod;
  const job = useJob("mod", entry.id);
  // The installed mod's own game version gets it.
  const start = useStartJob(() => commands.installIndexMod(entry.id, null, true, null));

  if (job && (!isFinished(job) || start.isSuccess)) return <JobRow job={job} label={entry.name} />;
  if (!mod.updateAvailable) return <Notice>{t("modUpdate.upToDate")}</Notice>;
  return (
    <>
      <Notice>
        {t("modUpdate.inIndex", {
          version: entry.version,
          context: entry.version ? "version" : undefined,
        })}
      </Notice>
      <div className="flex flex-wrap gap-2">
        <Button onClick={() => start.mutate()} disabled={disabled || start.isPending}>
          <RefreshCw className="size-4" aria-hidden />
          {t("modUpdate.update")}
        </Button>
        <Button variant="secondary" onClick={onDismiss} disabled={start.isPending}>
          {t("modUpdate.dismiss")}
        </Button>
      </div>
      {start.error && <Notice tone="error">{errorMessage(t, start.error)}</Notice>}
    </>
  );
}
