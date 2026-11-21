import { useMutation, useQueryClient } from "@tanstack/react-query";
import { CircleCheck, Copy, Pencil, Play, Star, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile, type World, type WorldStart } from "../bindings";
import { errorMessage } from "../errors";
import { formatDate } from "../format";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { ConfirmDialog } from "../ui/ConfirmDialog";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { RelativeTime } from "../ui/RelativeTime";
import { backupsKey, savesKey } from "./saves";
import { optionsSummary, useWorlds, worldName, worldsKey } from "./worlds";

/**
 * The worlds a mod that generates its levels has kept (§9.6): favourites
 * first, then by last start, each with its seed, options and saves.
 */
export function WorldsPanel({
  profile,
  canPlay,
  onPlay,
}: {
  profile: Profile;
  canPlay: boolean;
  onPlay: (world: WorldStart) => void;
}) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const worlds = useWorlds(profile);
  const [renaming, setRenaming] = useState<World | null>(null);
  const [deleting, setDeleting] = useState<World | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const refresh = () => {
    queryClient.invalidateQueries({ queryKey: worldsKey(profile.id) });
    queryClient.invalidateQueries({ queryKey: profilesKey });
  };

  const favourite = useMutation({
    mutationFn: (w: World) => unwrap(commands.setWorldFavourite(profile.id, w.id, !w.favourite)),
    onSuccess: refresh,
  });
  const rename = useMutation({
    mutationFn: ({ w, name }: { w: World; name: string }) =>
      unwrap(commands.renameWorld(profile.id, w.id, name.trim() || null)),
    onSuccess: () => {
      setRenaming(null);
      refresh();
    },
  });
  const remove = useMutation({
    mutationFn: (w: World) => unwrap(commands.deleteWorld(profile.id, w.id)),
    onSuccess: () => {
      setDeleting(null);
      setMessage(t("worlds.deleted"));
      refresh();
      queryClient.invalidateQueries({ queryKey: savesKey(profile.id) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profile.id) });
    },
  });
  const makeCurrent = useMutation({
    mutationFn: (w: World) => unwrap(commands.activateWorld(profile.id, w.id)),
    onSuccess: (_, w) => {
      setMessage(t("worlds.madeCurrent", { name: worldName(t, w) }));
      refresh();
      queryClient.invalidateQueries({ queryKey: savesKey(profile.id) });
    },
  });
  const copySeed = useMutation({
    mutationFn: (seed: number) => navigator.clipboard.writeText(String(seed)),
    onSuccess: () => setMessage(t("worlds.seedCopied")),
  });

  const data = worlds.data;
  const error =
    worlds.error ??
    favourite.error ??
    rename.error ??
    remove.error ??
    makeCurrent.error ??
    copySeed.error;
  const locked = profile.running;
  // Copying a seed is offered in every row as soon as one has a seed.
  const seeded = !!data?.worlds.some((w) => w.seed !== null);

  return (
    <div className="grid gap-4">
      {locked && <Notice>{t("worlds.locked")}</Notice>}
      {message && !error && <Notice>{message}</Notice>}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      <p className="text-sm text-muted">{t("worlds.intro")}</p>
      {data && data.worlds.length === 0 && (
        <p className="text-sm text-muted">{t("worlds.empty")}</p>
      )}
      <ul className="grid gap-2">
        {data?.worlds.map((w) => {
          const name = worldName(t, w);
          const options = optionsSummary(data.definition, w, i18n.language);
          // Named after its seed, the name says it already.
          const seed = w.name !== null ? w.seed : null;
          return (
            <li key={w.id}>
              <Card className="flex flex-wrap items-center gap-3 p-3">
                <button
                  type="button"
                  className={`rounded-lg p-1.5 hover:bg-surface-raised ${
                    w.favourite ? "text-warning" : "text-muted"
                  }`}
                  aria-pressed={w.favourite}
                  aria-label={t("worlds.favouriteNamed", { name })}
                  title={t("worlds.favourite")}
                  onClick={() => favourite.mutate(w)}
                >
                  <Star className={`size-4 ${w.favourite ? "fill-current" : ""}`} aria-hidden />
                </button>
                <div className="grid min-w-0 flex-1 gap-0.5">
                  {/* Next below the page's title: the tab has no heading of its own. */}
                  <h2 className="flex flex-wrap items-center gap-2 text-sm font-semibold">
                    <span className="select-text">{name}</span>
                    {w.active && <Badge>{t("worlds.playing")}</Badge>}
                  </h2>
                  {(seed !== null || options.length > 0) && (
                    <p className="text-xs text-muted">
                      {seed !== null && (
                        <>
                          {t("worlds.seed")}: <span className="select-text">{seed}</span>
                        </>
                      )}
                      {seed !== null && options.length > 0 && " · "}
                      {options.join(" · ")}
                    </p>
                  )}
                  <p className="text-xs text-muted">
                    {t("worlds.firstStart", { date: formatDate(i18n.language, w.createdAt) })}
                    {" · "}
                    <RelativeTime at={w.lastPlayedAt}>
                      {(date) => t("worlds.lastStart", { date })}
                    </RelativeTime>
                    {" · "}
                    {t("worlds.saves", { count: w.saves })}
                  </p>
                </div>
                <div className="flex gap-1">
                  <Button
                    variant="secondary"
                    disabled={!canPlay || locked}
                    onClick={() => onPlay({ type: "continue", id: w.id })}
                    aria-label={t("worlds.playNamed", { name })}
                  >
                    <Play className="size-4" aria-hidden />
                    {t("worlds.play")}
                  </Button>
                  {/* Greyed out rather than left out, so every row's
                      buttons stand in the same places. */}
                  <Button
                    variant="ghost"
                    className="px-2"
                    disabled={w.active || locked || makeCurrent.isPending}
                    onClick={() => makeCurrent.mutate(w)}
                    aria-label={t(w.active ? "worlds.isCurrentNamed" : "worlds.makeCurrentNamed", {
                      name,
                    })}
                    title={t(w.active ? "worlds.isCurrent" : "worlds.makeCurrent")}
                  >
                    <CircleCheck className="size-4" aria-hidden />
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2"
                    onClick={() => setRenaming(w)}
                    aria-label={t("worlds.renameNamed", { name })}
                    title={t("worlds.rename")}
                  >
                    <Pencil className="size-4" aria-hidden />
                  </Button>
                  {seeded && (
                    <Button
                      variant="ghost"
                      className="px-2"
                      disabled={w.seed === null}
                      onClick={() => copySeed.mutate(w.seed!)}
                      aria-label={t("worlds.copySeedNamed", { name })}
                      title={t("worlds.copySeed")}
                    >
                      <Copy className="size-4" aria-hidden />
                    </Button>
                  )}
                  <Button
                    variant="ghost"
                    className="px-2"
                    disabled={locked}
                    onClick={() => setDeleting(w)}
                    aria-label={t("worlds.deleteNamed", { name })}
                    title={t("worlds.delete")}
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </Button>
                </div>
              </Card>
            </li>
          );
        })}
      </ul>

      {renaming && (
        <RenameDialog
          world={renaming}
          busy={rename.isPending}
          onRename={(name) => rename.mutate({ w: renaming, name })}
          onClose={() => setRenaming(null)}
        />
      )}
      {deleting && (
        <ConfirmDialog
          title={t("worlds.deleteTitle", { name: worldName(t, deleting) })}
          confirm={t("worlds.delete")}
          busy={remove.isPending}
          onConfirm={() => remove.mutate(deleting)}
          onClose={() => setDeleting(null)}
        >
          <p className="text-sm">
            {deleting.active ? t("worlds.deleteActiveText") : t("worlds.deleteText")}
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}

function RenameDialog({
  world,
  busy,
  onRename,
  onClose,
}: {
  world: World;
  busy: boolean;
  onRename: (name: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(world.name ?? "");
  return (
    <Dialog title={t("worlds.renameTitle")} onClose={onClose}>
      <form
        className="grid gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          onRename(name);
        }}
      >
        <input
          className="rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm"
          autoComplete="off"
          maxLength={60}
          value={name}
          placeholder={worldName(t, { ...world, name: null })}
          aria-label={t("worlds.name")}
          onChange={(e) => setName(e.target.value)}
          data-autofocus
        />
        <p className="text-xs text-muted">{t("worlds.renameHint")}</p>
        <div className="flex justify-end gap-3">
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button type="submit" disabled={busy}>
            {t("worlds.save")}
          </Button>
        </div>
      </form>
    </Dialog>
  );
}
