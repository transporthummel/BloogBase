import { useMutation, useQuery } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type Backup,
  type FolderSaves,
  type Game,
  type SaveDir,
  type SaveFile,
} from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { backupTitle, folderName, placedSaveTitle, saveSummary, slotFiles } from "./saves";

type Shared = {
  game: Game;
  /** The folders the mod keeps saves in besides the root. */
  saveDirs: SaveDir[];
  current: SaveFile[];
  onClose: () => void;
  /** Whether what was there was backed up first. */
  onRestored: (backedUp: boolean) => void;
};

/** Restores a whole backup, or one save from it into a chosen slot. */
export function RestoreDialog({ backup, ...shared }: Shared & { backup: Backup }) {
  const { t } = useTranslation();
  const inBackup = useQuery({
    queryKey: ["backupSaves", backup.id],
    queryFn: () => unwrap(commands.backupSaves(backup.id)),
  });
  return (
    <RestoreFrom
      {...shared}
      title={t("backups.restoreTitle", { name: backupTitle(t, backup) })}
      saves={inBackup.data}
      error={inBackup.error}
      hasOther={backup.settings + backup.other > 0}
      restore={(file, target, all) => unwrap(commands.restoreBackup(backup.id, file, target, all))}
    />
  );
}

/**
 * Restores the saves lying in a folder, such as an old copy of the game,
 * all or one into a chosen slot, without keeping them as a backup. Where
 * the mod keeps saves in folders and this one doesn't say which, the
 * player picks the part first.
 */
export function FolderRestoreDialog({
  profileId,
  path,
  found,
  ...shared
}: Shared & { profileId: string; path: string; found: FolderSaves }) {
  const { t, i18n } = useTranslation();
  const name = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  const [part, setPart] = useState<string | null>(null);
  // Loose files go into the part chosen, as the backend puts them.
  const saves = found.saves.map((s) =>
    part !== null && !s.file.includes("/") ? { ...s, file: `${part}/${s.file}` } : s,
  );
  return (
    <RestoreFrom
      {...shared}
      title={t("saves.importFolderTitle", { name })}
      saves={saves}
      error={null}
      hasOther={found.settings > 0}
      blocked={found.loose && part === null}
      restore={(file, target, all) =>
        unwrap(commands.restoreFolder(profileId, path, part, file, target, all))
      }
    >
      {found.loose && (
        <div className="grid gap-1">
          <label className="flex items-center gap-2 text-sm">
            {t("saves.importPart")}
            <select
              className="rounded-lg border border-border bg-surface-raised px-2 py-1 text-sm"
              value={part ?? ""}
              onChange={(e) => setPart(e.target.value || null)}
            >
              <option value="">{t("saves.importPartPick")}</option>
              {shared.saveDirs.map((d) => (
                <option key={d.dir} value={d.dir}>
                  {folderName(shared.saveDirs, d.dir, i18n.language)}
                </option>
              ))}
            </select>
          </label>
          <p className="text-xs text-muted">{t("saves.importPartHint")}</p>
        </div>
      )}
    </RestoreFrom>
  );
}

/**
 * Restores all saves of a backup or folder, or one into a chosen slot.
 * Says which current saves get replaced; the backend backs them up first.
 */
function RestoreFrom({
  title,
  saves,
  error,
  hasOther,
  blocked = false,
  restore: put,
  children,
  game,
  saveDirs,
  current,
  onClose,
  onRestored,
}: Shared & {
  title: string;
  saves: SaveFile[] | undefined;
  error: Error | null;
  /** Settings or high scores to restore too, if asked. */
  hasOther: boolean;
  /** Nothing can be restored yet. */
  blocked?: boolean;
  /** Above the saves. */
  children?: ReactNode;
  restore: (file: string | null, target: string | null, all: boolean) => Promise<boolean>;
}) {
  const { t, i18n } = useTranslation();
  const inBackup = { data: saves, error };
  const named = (s: SaveFile, others: SaveFile[]) =>
    placedSaveTitle(t, i18n.language, saveDirs, s, others);
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
    }) => put(file, target, all),
    onSuccess: onRestored,
  });

  const occupied = (file: string) =>
    current.find((c) => c.file.toUpperCase() === file.toUpperCase());
  const replacedByAll = (inBackup.data ?? []).map((s) => occupied(s.file)).filter((s) => !!s);
  const onlyOther = hasOther && inBackup.data?.length === 0;

  return (
    <Dialog title={title} onClose={onClose}>
      {inBackup.error && <Notice tone="error">{errorMessage(t, inBackup.error)}</Notice>}
      {restore.error && <Notice tone="error">{errorMessage(t, restore.error)}</Notice>}
      {children}
      <ul className="grid gap-3">
        {inBackup.data?.map((s) => {
          const target = targets[s.file] ?? s.file;
          const replaced = occupied(target);
          return (
            <li key={s.file} className="grid gap-2 rounded-lg border border-border p-3">
              <div className="grid gap-0.5">
                <h3 className="text-sm font-semibold">{named(s, inBackup.data ?? [])}</h3>
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
                  disabled={blocked || restore.isPending}
                  onClick={() =>
                    restore.mutate({ file: s.file, target: target === s.file ? null : target })
                  }
                >
                  {t("backups.restoreOne")}
                </Button>
              </div>
              {replaced && (
                <p className="text-xs text-muted">
                  {t("backups.replaces", { name: named(replaced, current) })}
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
              disabled={blocked || restore.isPending}
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
