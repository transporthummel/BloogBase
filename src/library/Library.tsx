import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Navigate } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import { Compass, FileArchive, FolderOpen, Puzzle, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type Mod, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatDate, formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { AddModDialog } from "../mods/AddModDialog";
import { SuggestToIndex } from "../mods/KeenwikiImport";
import { RemoveModDialog } from "../mods/RemoveModDialog";
import { Cover } from "./Cover";
import { useCover } from "./covers";
import { installsKey, profilesKey } from "./queries";
import { getSetupStep } from "../preferences";
import { useRuntimeStatus } from "../runtime/queries";
import { useSetupStatus } from "../setup/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { PlayButton } from "../ui/PlayButton";

export function Library() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const setup = useSetupStatus();
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });
  const runtime = useRuntimeStatus();
  const [addingMod, setAddingMod] = useState(false);
  const [installedMod, setInstalledMod] = useState<Mod | null>(null);
  const [removing, setRemoving] = useState<(Profile & { modId: string }) | null>(null);

  useEffect(() => {
    const unlisten = events.playSessionEnded.listen(() =>
      queryClient.invalidateQueries({ queryKey: profilesKey }),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, [queryClient]);

  const importGames = useMutation({
    mutationFn: (path: string) => unwrap(commands.importGames(path)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: installsKey });
    },
  });

  const launch = useMutation({
    mutationFn: (profileId: string) => unwrap(commands.launchProfile(profileId)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: profilesKey }),
  });

  async function pick(kind: "folder" | "zip") {
    const path = await open(
      kind === "folder" ? { directory: true } : { filters: [{ name: "ZIP", extensions: ["zip"] }] },
    );
    if (path) importGames.mutate(path);
  }

  const importButtons = (
    <>
      <Button onClick={() => pick("folder")} disabled={importGames.isPending}>
        <FolderOpen className="size-4" aria-hidden />
        {t("library.importFolder")}
      </Button>
      <Button variant="secondary" onClick={() => pick("zip")} disabled={importGames.isPending}>
        <FileArchive className="size-4" aria-hidden />
        {t("library.importZip")}
      </Button>
    </>
  );

  // First start, or a wizard that was interrupted after adding games.
  if (setup.data && !setup.data.completed && (!setup.data.hasInstalls || getSetupStep()))
    return <Navigate to="/setup" />;

  const gameById = new Map(games.data?.map((g) => [g.id, g]));
  const error = importGames.error ?? launch.error ?? profiles.error;
  const empty = profiles.data?.length === 0;
  const needsRuntime = !empty && runtime.data && !runtime.data.active;

  return (
    <>
      <PageHeader
        title={t("library.title")}
        actions={
          !empty && (
            <>
              <Link
                to="/discover"
                className="inline-flex items-center justify-center gap-2 rounded-lg bg-accent px-4 py-2 text-sm font-semibold text-accent-ink shadow-card transition hover:brightness-110"
              >
                <Compass className="size-4" aria-hidden />
                {t("discover.find")}
              </Link>
              <Button variant="secondary" onClick={() => setAddingMod(true)}>
                <Puzzle className="size-4" aria-hidden />
                {t("mods.add")}
              </Button>
              {importButtons}
            </>
          )
        }
      />
      {addingMod && (
        <AddModDialog
          onClose={() => setAddingMod(false)}
          onInstalled={(mod) => {
            setAddingMod(false);
            setInstalledMod(mod);
            queryClient.invalidateQueries({ queryKey: profilesKey });
          }}
        />
      )}
      {removing && (
        <RemoveModDialog
          profile={removing}
          onClose={() => setRemoving(null)}
          onRemoved={() => {
            setRemoving(null);
            setInstalledMod(null);
            queryClient.invalidateQueries({ queryKey: profilesKey });
          }}
        />
      )}

      <div className="mb-4 grid gap-3">
        {needsRuntime && (
          <Card className="grid gap-3 p-5">
            <h2 className="font-semibold">{t("runtime.neededTitle")}</h2>
            <p className="text-sm text-muted">{t("runtime.needed")}</p>
            {runtime.data?.downloadAvailable ? (
              <InstallRuntimeButton
                label={t("runtime.install", { version: runtime.data.pinnedVersion })}
              />
            ) : (
              <p className="text-sm">
                <Link to="/settings" className="underline">
                  {t("runtime.chooseInSettings")}
                </Link>
              </p>
            )}
          </Card>
        )}
        {importGames.isPending && <Notice>{t("library.importing")}</Notice>}
        {importGames.isSuccess && (
          <Notice>{t("library.imported", { count: importGames.data.length })}</Notice>
        )}
        {installedMod && (
          <Notice>
            {t("mods.installed", { name: installedMod.name })}
            {installedMod.source === "keenwiki" && (
              <>
                {" "}
                <SuggestToIndex modId={installedMod.id} />
              </>
            )}
          </Notice>
        )}
        {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      </div>

      {empty && (
        <EmptyState title={t("library.emptyTitle")}>
          <p className="max-w-md text-sm text-muted">{t("library.empty")}</p>
          <div className="flex flex-wrap justify-center gap-3">{importButtons}</div>
        </EmptyState>
      )}

      <ul className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-4">
        {profiles.data?.map((profile) => (
          <li key={profile.id}>
            <ProfileCard
              profile={profile}
              game={gameById.get(profile.gameId)}
              canPlay={!!runtime.data?.active && !launch.isPending}
              onPlay={() => launch.mutate(profile.id)}
              onRemove={
                profile.modId === null
                  ? undefined
                  : () => setRemoving({ ...profile, modId: profile.modId! })
              }
            />
          </li>
        ))}
      </ul>
    </>
  );
}

function ProfileCard({
  profile,
  game,
  canPlay,
  onPlay,
  onRemove,
}: {
  profile: Profile;
  game?: Game;
  canPlay: boolean;
  onPlay: () => void;
  /** Only mods can be removed for now. */
  onRemove?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const cover = useCover(profile.id);
  return (
    <Card className="flex h-full flex-col">
      <Cover gameId={profile.gameId} game={game} src={cover.data} />
      <div className="flex flex-1 flex-col gap-2 p-3">
        <div className="grid gap-0.5">
          <h2 className="text-sm leading-tight font-semibold">
            <Link
              to="/profiles/$profileId"
              params={{ profileId: profile.id }}
              className="hover:underline"
              title={t("library.details")}
            >
              {profile.label}
            </Link>
          </h2>
          {profile.modId !== null && game && (
            <p className="text-xs text-muted">
              {t("mods.baseGame", {
                game: game.title,
                version: version?.label ?? t("library.unknownVersion"),
              })}
            </p>
          )}
        </div>
        <div className="flex flex-wrap gap-1">
          {profile.modId !== null && <Badge>{t("mods.badge")}</Badge>}
          {profile.modId !== null && profile.sessionCount === 0 && <Badge>{t("mods.new")}</Badge>}
          {profile.modId === null && <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>}
        </div>
        <dl className="grid gap-0.5 text-xs text-muted">
          <dt className="sr-only">{t("library.playTime")}</dt>
          <dd>
            {profile.sessionCount === 0
              ? t("library.neverPlayed")
              : t("library.played", { time: formatPlayTime(t, profile.playSeconds) })}
          </dd>
          {profile.lastPlayedAt !== null && (
            <>
              <dt className="sr-only">{t("library.lastPlayed")}</dt>
              <dd>
                {t("library.lastPlayedOn", {
                  date: formatDate(i18n.language, profile.lastPlayedAt),
                })}
              </dd>
            </>
          )}
          <dt className="sr-only">{t("library.saveCountLabel")}</dt>
          <dd>
            <Link
              to="/profiles/$profileId"
              params={{ profileId: profile.id }}
              className="underline-offset-2 hover:underline"
            >
              {t("library.saveCount", { count: profile.saveCount })}
            </Link>
          </dd>
        </dl>
        <div className="mt-auto flex gap-2">
          <PlayButton
            className="flex-1"
            onClick={onPlay}
            disabled={!canPlay || profile.running}
            aria-label={t("library.playGame", { game: profile.label })}
          >
            {profile.running ? t("library.running") : t("library.play")}
          </PlayButton>
          {onRemove && (
            <Button
              variant="ghost"
              className="px-2"
              onClick={onRemove}
              disabled={profile.running}
              aria-label={t("mods.removeNamed", { name: profile.label })}
              title={t("mods.remove")}
            >
              <Trash2 className="size-4" aria-hidden />
            </Button>
          )}
        </div>
      </div>
    </Card>
  );
}
