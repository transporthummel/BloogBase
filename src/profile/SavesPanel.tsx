import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { Archive, Download, History, Trash2, Upload } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Backup, type Game, type Profile, type SaveFile } from "../bindings";
import { errorMessage } from "../errors";
import { formatDateTime } from "../format";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Notice } from "../ui/Notice";
import { RestoreDialog } from "./RestoreDialog";
import { backupTitle, backupsKey, saveSummary, saveTitle, savesKey } from "./saves";

/** The save manager of one profile: its saves and their backups (§11). */
export function SavesPanel({ profile, game }: { profile: Profile; game?: Game }) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const saves = useQuery({
    queryKey: savesKey(profile.id),
    queryFn: () => unwrap(commands.listSaves(profile.id)),
  });
  const backups = useQuery({
    queryKey: backupsKey(profile.id),
    queryFn: () => unwrap(commands.listBackups(profile.id)),
  });
  const [deleting, setDeleting] = useState<SaveFile | null>(null);
  const [removingBackup, setRemovingBackup] = useState<Backup | null>(null);
  const [restoring, setRestoring] = useState<Backup | null>(null);
  const [label, setLabel] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: savesKey(profile.id) });
    queryClient.invalidateQueries({ queryKey: backupsKey(profile.id) });
    queryClient.invalidateQueries({ queryKey: profilesKey });
  };

  const backup = useMutation({
    mutationFn: (file: string | null) =>
      unwrap(commands.createBackup(profile.id, file ? null : label || null, file)),
    onSuccess: (made) => {
      setLabel("");
      setMessage(made ? t("backups.made") : t("backups.nothing"));
      refresh();
    },
  });
  const deleteSave = useMutation({
    mutationFn: (file: string) => unwrap(commands.deleteSave(profile.id, file)),
    onSuccess: () => {
      setDeleting(null);
      setMessage(t("saves.deleted"));
      refresh();
    },
  });
  const deleteBackup = useMutation({
    mutationFn: (id: number) => unwrap(commands.deleteBackup(id)),
    onSuccess: () => {
      setRemovingBackup(null);
      refresh();
    },
  });
  const exportBackup = useMutation({
    mutationFn: async (b: Backup) => {
      const date = new Date(b.createdAt * 1000).toISOString().slice(0, 10);
      const path = await saveDialog({
        defaultPath: `${profile.id}-${date}.zip`,
        filters: [{ name: "ZIP", extensions: ["zip"] }],
      });
      if (!path) return false;
      await unwrap(commands.exportBackup(b.id, path));
      return true;
    },
    onSuccess: (done) => done && setMessage(t("backups.exported")),
  });
  const importBackup = useMutation({
    mutationFn: async () => {
      const path = await open({ filters: [{ name: "ZIP", extensions: ["zip"] }] });
      if (!path) return false;
      await unwrap(commands.importBackup(profile.id, path));
      return true;
    },
    onSuccess: (done) => {
      if (!done) return;
      setMessage(t("backups.imported"));
      refresh();
    },
  });

  const locked = profile.running;
  const error =
    saves.error ??
    backups.error ??
    backup.error ??
    deleteSave.error ??
    exportBackup.error ??
    importBackup.error ??
    deleteBackup.error;

  return (
    <div className="grid gap-6">
      {locked && <Notice>{t("saves.locked")}</Notice>}
      {message && !error && <Notice>{message}</Notice>}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}

      <section className="grid gap-3" aria-labelledby="saves-heading">
        <h2 id="saves-heading" className="font-semibold">
          {t("saves.title")}
        </h2>
        {saves.data?.length === 0 && <p className="text-sm text-muted">{t("saves.empty")}</p>}
        <ul className="grid gap-2">
          {saves.data?.map((s) => (
            <li key={s.file}>
              <Card className="flex flex-wrap items-center gap-3 p-3">
                <div className="grid min-w-0 flex-1 gap-0.5">
                  <h3 className="text-sm font-semibold">{saveTitle(t, s)}</h3>
                  <p className="text-xs text-muted">{saveSummary(t, i18n.language, s)}</p>
                  {s.modifiedAt !== null && (
                    <p className="text-xs text-muted">
                      {t("saves.savedAt", { date: formatDateTime(i18n.language, s.modifiedAt) })}
                    </p>
                  )}
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="ghost"
                    className="px-2"
                    disabled={locked || backup.isPending}
                    onClick={() => backup.mutate(s.file)}
                    aria-label={t("saves.backupNamed", { name: saveTitle(t, s) })}
                    title={t("saves.backup")}
                  >
                    <Archive className="size-4" aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2"
                    disabled={locked}
                    onClick={() => setDeleting(s)}
                    aria-label={t("saves.deleteNamed", { name: saveTitle(t, s) })}
                    title={t("saves.delete")}
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </Button>
                </div>
              </Card>
            </li>
          ))}
        </ul>
      </section>

      <section className="grid gap-3" aria-labelledby="backups-heading">
        <h2 id="backups-heading" className="font-semibold">
          {t("backups.title")}
        </h2>
        <form
          className="flex flex-wrap gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            backup.mutate(null);
          }}
        >
          <input
            className="min-w-0 flex-1 rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm"
            value={label}
            maxLength={80}
            onChange={(e) => setLabel(e.target.value)}
            placeholder={t("backups.labelPlaceholder")}
            aria-label={t("backups.label")}
          />
          <Button type="submit" disabled={locked || backup.isPending}>
            <Archive className="size-4" aria-hidden />
            {t("backups.backupAll")}
          </Button>
          <Button
            variant="secondary"
            onClick={() => importBackup.mutate()}
            disabled={importBackup.isPending}
          >
            <Upload className="size-4" aria-hidden />
            {t("backups.import")}
          </Button>
        </form>
        {backups.data?.length === 0 && <p className="text-sm text-muted">{t("backups.empty")}</p>}
        <ul className="grid gap-2">
          {backups.data?.map((b) => (
            <li key={b.id}>
              <Card className="flex flex-wrap items-center gap-3 p-3">
                <div className="grid min-w-0 flex-1 gap-0.5">
                  <h3 className="flex flex-wrap items-center gap-2 text-sm font-semibold">
                    {backupTitle(t, b)}
                    {b.auto && <Badge>{t("backups.auto")}</Badge>}
                  </h3>
                  <p className="text-xs text-muted">
                    {formatDateTime(i18n.language, b.createdAt)} ·{" "}
                    {t("backups.files", { count: b.files.length })}
                  </p>
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="secondary"
                    disabled={locked}
                    onClick={() => setRestoring(b)}
                    aria-label={t("backups.restoreNamed", { name: backupTitle(t, b) })}
                  >
                    <History className="size-4" aria-hidden />
                    {t("backups.restore")}
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2"
                    onClick={() => exportBackup.mutate(b)}
                    aria-label={t("backups.exportNamed", { name: backupTitle(t, b) })}
                    title={t("backups.export")}
                  >
                    <Download className="size-4" aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2"
                    onClick={() => setRemovingBackup(b)}
                    aria-label={t("backups.deleteNamed", { name: backupTitle(t, b) })}
                    title={t("backups.delete")}
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </Button>
                </div>
              </Card>
            </li>
          ))}
        </ul>
      </section>

      {deleting && (
        <ConfirmDialog
          title={t("saves.deleteTitle", { name: saveTitle(t, deleting) })}
          confirm={t("saves.delete")}
          busy={deleteSave.isPending}
          onConfirm={() => deleteSave.mutate(deleting.file)}
          onClose={() => setDeleting(null)}
        >
          <p className="text-sm text-muted">{t("saves.deleteHint")}</p>
        </ConfirmDialog>
      )}
      {removingBackup && (
        <ConfirmDialog
          title={t("backups.deleteTitle")}
          confirm={t("backups.delete")}
          busy={deleteBackup.isPending}
          onConfirm={() => deleteBackup.mutate(removingBackup.id)}
          onClose={() => setRemovingBackup(null)}
        >
          <p className="text-sm text-muted">
            {t("backups.deleteHint", {
              name: backupTitle(t, removingBackup),
              date: formatDateTime(i18n.language, removingBackup.createdAt),
            })}
          </p>
        </ConfirmDialog>
      )}
      {restoring && game && (
        <RestoreDialog
          backup={restoring}
          game={game}
          current={saves.data ?? []}
          onClose={() => setRestoring(null)}
          onRestored={() => {
            setRestoring(null);
            setMessage(t("backups.restored"));
            refresh();
          }}
        />
      )}
    </div>
  );
}
