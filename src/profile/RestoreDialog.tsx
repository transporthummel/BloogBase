import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Backup, type Game, type SaveFile } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { backupTitle, saveSummary, saveTitle, slotFiles } from "./saves";

/**
 * Restores a whole backup, or one save from it into a chosen slot. Says
 * which current saves get replaced; the backend backs them up first.
 */
export function RestoreDialog({
  backup,
  game,
  current,
  onClose,
  onRestored,
}: {
  backup: Backup;
  game: Game;
  current: SaveFile[];
  onClose: () => void;
  onRestored: () => void;
}) {
  const { t, i18n } = useTranslation();
  const inBackup = useQuery({
    queryKey: ["backupSaves", backup.id],
    queryFn: () => unwrap(commands.backupSaves(backup.id)),
  });
  const [targets, setTargets] = useState<Record<string, string>>({});
  // Settings and high scores stay as they are unless asked.
  const [includeOther, setIncludeOther] = useState(false);
  const restore = useMutation({
    mutationFn: ({
      file,
      target,
      all = false,
    }: {
      file: string | null;
      target: string | null;
      all?: boolean;
    }) => unwrap(commands.restoreBackup(backup.id, file, target, all)),
    onSuccess: onRestored,
  });

  const occupied = (file: string) =>
    current.find((c) => c.file.toUpperCase() === file.toUpperCase());
  const replacedByAll = (inBackup.data ?? []).map((s) => occupied(s.file)).filter((s) => !!s);
  const hasOther = backup.settings + backup.other > 0;
  const onlyOther = hasOther && inBackup.data?.length === 0;

  return (
    <Dialog title={t("backups.restoreTitle", { name: backupTitle(t, backup) })} onClose={onClose}>
      {inBackup.error && <Notice tone="error">{errorMessage(t, inBackup.error)}</Notice>}
      {restore.error && <Notice tone="error">{errorMessage(t, restore.error)}</Notice>}
      <ul className="grid gap-3">
        {inBackup.data?.map((s) => {
          const target = targets[s.file] ?? s.file;
          const replaced = occupied(target);
          return (
            <li key={s.file} className="grid gap-2 rounded-lg border border-border p-3">
              <div className="grid gap-0.5">
                <h3 className="text-sm font-semibold">{saveTitle(t, s)}</h3>
                <p className="text-xs text-muted">{saveSummary(t, i18n.language, s)}</p>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <label className="flex items-center gap-2 text-sm">
                  {t("backups.intoSlot")}
                  <select
                    className="rounded-lg border border-border bg-surface-raised px-2 py-1 text-sm"
                    value={target}
                    onChange={(e) => setTargets({ ...targets, [s.file]: e.target.value })}
                  >
                    {slotFiles(game.engine, s.file).map((o) => (
                      <option key={o.file} value={o.file}>
                        {t("saves.slot", { slot: o.slot })}
                      </option>
                    ))}
                  </select>
                </label>
                <Button
                  variant="secondary"
                  disabled={restore.isPending}
                  onClick={() =>
                    restore.mutate({ file: s.file, target: target === s.file ? null : target })
                  }
                >
                  {t("backups.restoreOne")}
                </Button>
              </div>
              {replaced && (
                <p className="text-xs text-muted">
                  {t("backups.replaces", { name: saveTitle(t, replaced) })}
                </p>
              )}
            </li>
          );
        })}
      </ul>
      {inBackup.data && (
        <div className="grid gap-2 border-t border-border pt-3">
          <p className="text-sm text-muted">
            {onlyOther
              ? t("backups.onlyOther")
              : replacedByAll.length > 0
                ? t("backups.restoreAllReplaces", { count: replacedByAll.length })
                : t("backups.restoreAllHint")}
          </p>
          {hasOther && !onlyOther && (
            <label className="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                className="mt-1"
                checked={includeOther}
                onChange={(e) => setIncludeOther(e.target.checked)}
              />
              <span>
                {t("backups.includeOther")}
                <span className="block text-xs text-muted">{t("backups.includeOtherHint")}</span>
              </span>
            </label>
          )}
          <div className="flex justify-end gap-3">
            <Button variant="ghost" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              disabled={restore.isPending}
              onClick={() =>
                restore.mutate({ file: null, target: null, all: onlyOther || includeOther })
              }
            >
              {onlyOther
                ? t("backups.restore")
                : includeOther
                  ? t("backups.restoreEverything")
                  : t("backups.restoreAll")}
            </Button>
          </div>
        </div>
      )}
    </Dialog>
  );
}
