import { useMutation, useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { commands, type SaveFile } from "../bindings";
import { errorMessage } from "../errors";
import { formatDateTime } from "../format";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { saveSummary, saveTitle } from "./saves";

/**
 * The earlier versions of one save that backups hold (§11.1); restoring one
 * puts it into this slot, after the backend backed up the current one.
 */
export function VersionsDialog({
  profileId,
  save,
  onClose,
  onRestored,
}: {
  profileId: string;
  save: SaveFile;
  onClose: () => void;
  onRestored: () => void;
}) {
  const { t, i18n } = useTranslation();
  const versions = useQuery({
    queryKey: ["saveVersions", profileId, save.file],
    queryFn: () => unwrap(commands.saveVersions(profileId, save.file)),
  });
  const restore = useMutation({
    mutationFn: ({ backupId, file }: { backupId: number; file: string }) =>
      unwrap(commands.restoreBackup(backupId, file, null, false)),
    onSuccess: onRestored,
  });

  return (
    <Dialog title={t("saves.versionsNamed", { name: saveTitle(t, save) })} onClose={onClose}>
      <p className="text-sm text-muted">{t("saves.versionsHint")}</p>
      {versions.error && <Notice tone="error">{errorMessage(t, versions.error)}</Notice>}
      {restore.error && <Notice tone="error">{errorMessage(t, restore.error)}</Notice>}
      <ul className="grid gap-3">
        {versions.data?.map((v) => {
          const date =
            v.save.modifiedAt === null ? "" : formatDateTime(i18n.language, v.save.modifiedAt);
          return (
            <li
              key={v.backupId}
              className="flex flex-wrap items-center gap-3 rounded-lg border border-border p-3"
            >
              <div className="grid min-w-0 flex-1 gap-0.5">
                <h3 className="text-sm font-semibold">{saveTitle(t, v.save)}</h3>
                <p className="text-xs text-muted">{saveSummary(t, i18n.language, v.save)}</p>
                <p className="text-xs text-muted">{t("saves.versionFrom", { date })}</p>
              </div>
              <Button
                variant="secondary"
                disabled={restore.isPending}
                onClick={() => restore.mutate({ backupId: v.backupId, file: v.save.file })}
                aria-label={t("saves.restoreVersionNamed", { date })}
              >
                {t("saves.restoreVersion")}
              </Button>
            </li>
          );
        })}
      </ul>
      <div className="flex justify-end">
        <Button variant="ghost" onClick={onClose}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}
