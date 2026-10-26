import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, Navigate, useNavigate } from "@tanstack/react-router";
import { Compass, Plus } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type Install, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { AddModDialog } from "../mods/AddModDialog";
import { RunsOn } from "../mods/RunsOn";
import { Cover } from "./Cover";
import { useCover } from "./covers";
import { profilesKey } from "./queries";
import { getLibrarySort, getShowTrivia, setLibrarySort } from "../preferences";
import { arrange, librarySorts, type LibrarySort } from "./sort";
import { StatusBar } from "./StatusBar";
import { leaveLibrary, returnToLibrary } from "./scroll";
import { startupTrivia, triviaText } from "./trivia";
import { useRuntimeStatus } from "../runtime/queries";
import { useSetupStatus } from "../setup/queries";
import { Badge } from "../ui/Badge";
import { Bloog } from "../ui/Bloog";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { RelativeTime } from "../ui/RelativeTime";
import { PlayButton } from "../ui/PlayButton";

export function Library() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const games = useGames();
  const setup = useSetupStatus();
  const profilesQuery = { queryKey: profilesKey, queryFn: () => unwrap(commands.listProfiles()) };
  const profiles = useQuery(profilesQuery);
  const runtime = useRuntimeStatus();
  const [adding, setAdding] = useState(false);
  const [imported, setImported] = useState<number | null>(null);
  const [sort, setSort] = useState(getLibrarySort);
  // Only picked (and remembered as seen) when it's shown.
  const [trivia] = useState(() => (getShowTrivia() ? startupTrivia() : null));
  const list = useRef<HTMLDivElement>(null);
  // Once the cards are laid out, so the old position means the same.
  const loaded = !!profiles.data && !!games.data;
  useLayoutEffect(() => {
    if (loaded && list.current) returnToLibrary(list.current);
  }, [loaded]);

  useEffect(() => {
    const unlisten = events.playSessionEnded.listen(() =>
      queryClient.invalidateQueries({ queryKey: profilesKey }),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, [queryClient]);

  const launch = useMutation({
    mutationFn: (profileId: string) => unwrap(commands.launchProfile(profileId)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: profilesKey }),
  });

  /** A single game imported opens right away; several are counted. */
  async function showImported(installs: Install[]) {
    if (installs.length === 1) {
      const all = await queryClient.fetchQuery(profilesQuery).catch(() => []);
      // The game's own play profile, not one of a mod played on it.
      const profile = all.find((p) => p.installId === installs[0].id && !p.modId);
      if (profile) {
        navigate({ to: "/profiles/$profileId", params: { profileId: profile.id } });
        return;
      }
    }
    setImported(installs.length);
  }

  const addButton = (
    <Button
      onClick={() => {
        setImported(null);
        setAdding(true);
      }}
    >
      <Plus className="size-4" aria-hidden />
      {t("library.add")}
    </Button>
  );

  // First start, or a wizard that was interrupted after adding games.
  if (setup.data && !setup.data.completed && (!setup.data.hasInstalls || setup.data.step))
    return <Navigate to="/setup" />;

  const gameById = new Map(games.data?.map((g) => [g.id, g]));
  const error = launch.error ?? profiles.error;
  const empty = profiles.data?.length === 0;
  const needsRuntime = !empty && runtime.data && !runtime.data.active;
  const groups = arrange(profiles.data ?? [], sort, games.data ?? [], i18n.language);

  return (
    <>
      <PageHeader title={t("library.title")} actions={!empty && addButton} />
      {trivia && (
        // Dashed, with a Bloog and the pixel font, so it reads as a joke and not as news.
        <aside className="-mt-2 mb-6 flex items-center gap-3 rounded-lg border border-dashed border-border bg-surface/60 px-4 py-2.5">
          <Bloog className="size-6 shrink-0" />
          <p className="text-sm text-muted">
            <span className="mr-2 font-pixel text-[0.6rem] text-accent uppercase">
              {t("library.trivia")}
            </span>
            <span className="italic">{triviaText(trivia, i18n.language)}</span>
          </p>
        </aside>
      )}
      {adding && (
        <AddModDialog
          onClose={() => setAdding(false)}
          onInstalled={(mod) => {
            setAdding(false);
            queryClient.invalidateQueries({ queryKey: profilesKey });
            navigate({ to: "/profiles/$profileId", params: { profileId: mod.profileId } });
          }}
          onGamesImported={(installs) => {
            setAdding(false);
            showImported(installs);
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
        {imported !== null && <Notice>{t("library.imported", { count: imported })}</Notice>}
        {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      </div>

      {empty && (
        <EmptyState title={t("library.emptyTitle")}>
          <p className="max-w-md text-sm text-muted">{t("library.empty")}</p>
          <div className="flex flex-wrap justify-center gap-3">
            {addButton}
            <Link
              to="/discover"
              search={{ tab: "games" }}
              className="inline-flex items-center justify-center gap-2 rounded-lg border border-border bg-surface-raised px-4 py-2 text-sm font-semibold text-ink transition hover:border-muted"
            >
              <Compass className="size-4" aria-hidden />
              {t("library.freeGames")}
            </Link>
          </div>
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

      <div ref={list} className="grid gap-8">
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
                  <li
                    key={profile.id}
                    data-profile-id={profile.id}
                    onClickCapture={(e) => {
                      if ((e.target as Element).closest("a")) leaveLibrary(profile.id);
                    }}
                  >
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
  const { t } = useTranslation();
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const cover = useCover(profile.id, profile.installId);
  return (
    // Only the border colour changes, so nothing inside the card moves.
    <Card className="flex h-full flex-col transition-colors has-[a:hover]:border-accent/35">
      {/* A second way to the same page for the mouse; the title link below
          is the one for keyboards and screen readers. */}
      <Link
        to="/profiles/$profileId"
        params={{ profileId: profile.id }}
        tabIndex={-1}
        aria-hidden
        className="block"
      >
        <Cover gameId={profile.gameId} game={game} src={cover.data?.src} />
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
                <RunsOn
                  profile={profile}
                  game={game.shortTitle}
                  version={version?.label ?? t("library.unknownVersion")}
                />
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
                <RelativeTime at={profile.lastPlayedAt}>
                  {(date) => t("library.lastPlayedOn", { date })}
                </RelativeTime>
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
        <div className="mt-auto flex gap-2 pt-1.5">
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
