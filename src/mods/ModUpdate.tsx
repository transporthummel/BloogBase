import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Download, RefreshCw } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Mod, type ModUpdate as Update } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { AddModDialog } from "./AddModDialog";
import { downloaded, type Downloaded } from "./wikiDownload";

/** Checks a mod's KeenWiki page for a newer version and installs it over the old one. */
export function ModUpdate({ mod, disabled }: { mod: Mod; disabled: boolean }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const check = useMutation({
    mutationFn: () => unwrap(commands.checkModUpdate(mod.id)),
  });
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

  return (
    <div className="grid justify-items-start gap-2">
      <Button
        variant="secondary"
        onClick={() => check.mutate()}
        disabled={check.isPending || download.isPending}
      >
        <RefreshCw className="size-4" aria-hidden />
        {t("modUpdate.check")}
      </Button>
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
      {download.error && <Notice tone="error">{errorMessage(t, download.error)}</Notice>}
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
