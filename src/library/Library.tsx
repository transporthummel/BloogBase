import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Navigate } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import { Compass, FileArchive, FolderOpen, Puzzle } from "lucide-react";
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
import { Cover } from "./Cover";
import { useCover } from "./covers";
import { installsKey, profilesKey } from "./queries";
import { getLibrarySort, getSetupStep, setLibrarySort } from "../preferences";
import { arrange, librarySorts, type LibrarySort } from "./sort";
import { StatusBar } from "./StatusBar";
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
  const { t, i18n } = useTranslation();
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
  const [sort, setSort] = useState(getLibrarySort);

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
  const groups = arrange(profiles.data ?? [], sort, games.data ?? [], i18n.language);

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

      {profiles.data && profiles.data.length > 0 && (
        <div className="mb-4 flex justify-end">
          <label className="flex items-center gap-2 text-sm text-muted">
            {t("library.sortBy")}
            <select
              className="rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm text-ink"
              value={sort}
              onChange={(e) => {
                const value = e.target.value as LibrarySort;
                setSort(value);
                setLibrarySort(value);
              }}
            >
              {librarySorts.map((s) => (
                <option key={s} value={s}>
                  {t(`library.sort.${s}`)}
                </option>
              ))}
            </select>
          </label>
        </div>
      )}

      <div className="grid gap-8">
        {groups.map((group) => {
          const grouped = group.key !== "all";
          const title =
            group.key === "games" || group.key === "mods"
              ? t(`library.group.${group.key}`)
              : (gameById.get(group.key)?.title ?? group.key);
          return (
            <section key={group.key} aria-label={grouped ? title : undefined}>
              {grouped && (
                <h2 className="mb-3 flex items-center gap-2 font-semibold">
                  {title}
                  <span className="min-w-6 rounded-full border border-border bg-surface-raised px-2 text-center font-pixel text-[0.6rem] leading-5 text-muted">
                    {group.profiles.length}
                  </span>
                </h2>
              )}
              <ul className="grid grid-cols-[repeat(auto-fill,minmax(11rem,1fr))] gap-4">
                {group.profiles.map((profile) => (
                  <li key={profile.id}>
                    <ProfileCard
                      profile={profile}
                      game={gameById.get(profile.gameId)}
                      heading={grouped ? "h3" : "h2"}
                      canPlay={!!runtime.data?.active && !launch.isPending}
                      onPlay={() => launch.mutate(profile.id)}
                    />
                  </li>
                ))}
              </ul>
            </section>
          );
        })}
      </div>
      {profiles.data && profiles.data.length > 0 && (
        <StatusBar profiles={profiles.data} updatedAt={profiles.dataUpdatedAt} />
      )}
    </>
  );
}

function ProfileCard({
  profile,
  game,
  heading: Heading,
  canPlay,
  onPlay,
}: {
  profile: Profile;
  game?: Game;
  /** h3 under a group heading. */
  heading: "h2" | "h3";
  canPlay: boolean;
  onPlay: () => void;
}) {
  const { t, i18n } = useTranslation();
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const cover = useCover(profile.id, profile.installId);
  return (
    <Card className="flex h-full flex-col">
      {/* A second way to the same page for the mouse; the title link below
          is the one for keyboards and screen readers. */}
      <Link
        to="/profiles/$profileId"
        params={{ profileId: profile.id }}
        tabIndex={-1}
        aria-hidden
        className="block transition hover:brightness-110"
      >
        <Cover gameId={profile.gameId} game={game} src={cover.data} />
      </Link>
      <div className="flex flex-1 flex-col gap-2 p-3">
        <div className="grid gap-0.5">
          <Heading className="text-sm leading-tight font-semibold">
            <Link
              to="/profiles/$profileId"
              params={{ profileId: profile.id }}
              className="hover:underline"
              title={t("library.details")}
            >
              {profile.label}
            </Link>
          </Heading>
          {profile.missingGame !== null ? (
            <p className="text-xs text-muted">
              {t("mods.waitsFor", { game: profile.missingGame })}
            </p>
          ) : (
            profile.modId !== null &&
            game && (
              <p className="text-xs text-muted">
                {t("mods.baseGame", {
                  game: game.title,
                  version: version?.label ?? t("library.unknownVersion"),
                })}
              </p>
            )
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
            disabled={!canPlay || profile.running || profile.missingGame !== null}
            aria-label={t("library.playGame", { game: profile.label })}
          >
            {profile.missingGame !== null
              ? t("mods.gameMissing")
              : profile.running
                ? t("library.running")
                : t("library.play")}
          </PlayButton>
        </div>
      </div>
    </Card>
  );
}
