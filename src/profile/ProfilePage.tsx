import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowLeft,
  ArrowLeftRight,
  ChevronDown,
  Download,
  ExternalLink,
  FileArchive,
  FolderOpen,
  Info,
  Keyboard,
  MonitorCog,
  PackagePlus,
  RefreshCw,
  Trash2,
  TriangleAlert,
} from "lucide-react";
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  commands,
  events,
  type Game,
  type IndexEntry,
  type Install,
  type Mod,
  type Profile,
} from "../bindings";
import { useGames } from "../catalog";
import { useFreeGames, useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { formatDate, formatLooseDate, formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { Cover } from "../library/Cover";
import { FindOwnedDialog } from "../library/FindOwnedDialog";
import { forgetRemovedCovers, useCover } from "../library/covers";
import { scroller } from "../library/scroll";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { Authors } from "../mods/Authors";
import { AddModDialog } from "../mods/AddModDialog";
import { SuggestToIndex } from "../mods/KeenwikiImport";
import { ModUpdate } from "../mods/ModUpdate";
import { useModUpdateCheck } from "../mods/modUpdateCheck";
import { RemoveModDialog } from "../mods/RemoveModDialog";
import { RunsOn } from "../mods/RunsOn";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { buttonClass } from "../ui/buttonClass";
import { Card } from "../ui/Card";
import { Dialog } from "../ui/Dialog";
import { Menu, type MenuItem } from "../ui/Menu";
import { Notice } from "../ui/Notice";
import { PlayButton } from "../ui/PlayButton";
import { RelativeTime } from "../ui/RelativeTime";
import { Tabs } from "../ui/Tabs";
import { ConfigPanel } from "./ConfigPanel";
import { oneButtonFiringKey, profileFiringKey } from "./firing";
import { ReadmePanel } from "./ReadmePanel";
import { RemoveGameDialog } from "./RemoveGameDialog";
import { useReadmes } from "./readme";
import { SavesPanel } from "./SavesPanel";
import { ScreenshotsPanel } from "./ScreenshotsPanel";
import { VideosPanel } from "./VideosPanel";
import { screenshotSrc, screenshotsKey, useScreenshots, useVideos, videosKey } from "./captures";
import { usePictureWidth } from "./heroPicture";
import { backupsKey, savesKey, useSaves } from "./saves";
import { usePageTitle } from "../ui/usePageTitle";

type Tab = "readme" | "saves" | "screenshots" | "videos" | "config";

export function ProfileRoute() {
  const { profileId } = useParams({ from: "/profiles/$profileId" });
  // Keyed, so going from one profile straight to another (a mod played over
  // another links to it) starts on its first tab again.
  return <ProfileDetail key={profileId} profileId={profileId} />;
}

/** One game or mod: play it, read about it, manage saves, screenshots, videos and settings (§8.3). */
export function ProfileDetail({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const runtime = useRuntimeStatus();
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });
  // Until the player picks one: the README if the mod has one, else saves.
  const [picked, setTab] = useState<Tab | null>(null);

  // The page shares its scrolling area with the library, which may have been far down.
  useLayoutEffect(() => {
    const main = scroller();
    if (main) main.scrollTop = 0;
  }, [profileId]);

  useEffect(() => {
    // The game may have saved, made an automatic backup before, or the
    // player took screenshots or videos.
    const unlisten = events.playSessionEnded.listen(() => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: savesKey(profileId) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profileId) });
      queryClient.invalidateQueries({ queryKey: screenshotsKey(profileId) });
      queryClient.invalidateQueries({ queryKey: videosKey(profileId) });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [queryClient, profileId]);

  const launch = useMutation({
    mutationFn: () => unwrap(commands.launchProfile(profileId)),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: backupsKey(profileId) });
    },
  });

  const profile = profiles.data?.find((p) => p.id === profileId);
  const game = games.data?.find((g) => g.id === profile?.gameId);
  const readmes = useReadmes(profile?.modId ?? null);
  const saves = useSaves(profileId);
  const shots = useScreenshots(profileId);
  const videos = useVideos(profileId);

  // A mod just installed opens before the list has been fetched again.
  if (profiles.data && !profile && !profiles.isFetching) {
    return (
      <>
        <BackLink />
        <Notice tone="error">{t("profile.notFound")}</Notice>
      </>
    );
  }

  const tabs: { id: Tab; label: ReactNode }[] = [
    ...(readmes.data?.length ? [{ id: "readme" as const, label: t("profile.tabReadme") }] : []),
    { id: "saves", label: <Counted label={t("profile.tabSaves")} items={saves.data} /> },
    {
      id: "screenshots",
      label: <Counted label={t("profile.tabScreenshots")} items={shots.data} />,
    },
    { id: "videos", label: <Counted label={t("profile.tabVideos")} items={videos.data} /> },
    { id: "config", label: t("profile.tabConfig") },
  ];
  const tab = tabs.find((x) => x.id === picked)?.id ?? tabs[0].id;
  // Wait for the README so the first tab doesn't change under the player.
  const tabsReady = !(profile?.modId && readmes.isPending);

  return (
    <>
      <BackLink />
      {profile && (
        <Hero
          profile={profile}
          game={game}
          canPlay={!!runtime.data?.active && !launch.isPending}
          onPlay={() => launch.mutate()}
          modCount={
            profiles.data?.filter((p) => p.modId !== null && p.installId === profile.installId)
              .length ?? 0
          }
        />
      )}
      {launch.error && (
        <div className="mb-4">
          <Notice tone="error">{errorMessage(t, launch.error)}</Notice>
        </div>
      )}

      {profile && tabsReady && (
        <Tabs tabs={tabs} selected={tab} onSelect={setTab} label={t("profile.tabs")}>
          <div className={tab === "screenshots" || tab === "videos" ? "" : "max-w-3xl"}>
            {profile.modId && readmes.data && tab === "readme" && (
              <>
                {/* Readmes tell players of their time to run batch files and set up DOSBox. */}
                <p className="mb-3 flex items-start gap-2 text-sm text-muted">
                  <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
                  {t("readme.handled")}
                </p>
                <ReadmePanel key={profile.id} modId={profile.modId} readmes={readmes.data} />
              </>
            )}
            {tab === "saves" && <SavesPanel profile={profile} game={game} />}
            {tab === "screenshots" && <ScreenshotsPanel profile={profile} />}
            {tab === "videos" && <VideosPanel profile={profile} />}
            {tab === "config" && <ConfigPanel profileId={profile.id} />}
          </div>
        </Tabs>
      )}
    </>
  );
}

/** A tab's name with how many there are, if any. */
function Counted({ label, items }: { label: string; items?: unknown[] }) {
  return (
    <>
      {label}
      {!!items?.length && (
        <>
          {" "}
          <Badge>{items.length}</Badge>
        </>
      )}
    </>
  );
}

/** Picture, title, credits, description, links and stats; Play, and the rest in a menu. */
function Hero({
  profile,
  game,
  canPlay,
  onPlay,
  modCount,
}: {
  profile: Profile;
  game?: Game;
  canPlay: boolean;
  onPlay: () => void;
  /** For a game: the mods played on it. */
  modCount: number;
}) {
  const { t, i18n } = useTranslation();
  usePageTitle(profile.label);
  const mods = useQuery({
    queryKey: modsKey,
    queryFn: () => unwrap(commands.listMods()),
    enabled: profile.modId !== null,
  });
  const index = useIndexMods();
  const shots = useScreenshots(profile.id);
  const cover = useCover(profile.id, profile.installId);
  const cardRef = useRef<HTMLElement>(null);
  const textRef = useRef<HTMLDivElement>(null);
  const pictureWidth = usePictureWidth(cardRef, textRef);
  const openFolder = useMutation({
    mutationFn: (modId: string) => unwrap(commands.openModFolder(modId)),
  });
  const [removing, setRemoving] = useState(false);
  const [replacing, setReplacing] = useState(false);
  const [picking, setPicking] = useState(false);
  const installs = useQuery({
    // Under the installs key, so it follows changes to the library.
    queryKey: [...installsKey, "mod", profile.modId],
    queryFn: () => unwrap(commands.modInstalls(profile.modId!)),
    enabled: profile.modId !== null,
  });
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  const mod = mods.data?.find((m) => m.id === profile.modId);
  const check = useModUpdateCheck(mod);
  // The index or the KeenWiki page may have a newer version.
  const checkable = mod && (mod.indexId || mod.keenwikiUrl);
  // Mods played over this one, as level packs for it.
  const extensions = mod ? (mods.data ?? []).filter((m) => m.extends === mod.id) : [];
  const entry = mod?.indexId
    ? index.data?.find((x) => x.entry.id === mod.indexId)?.entry
    : undefined;
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const latest = shots.data?.[0];
  // A translation from the index wins over the English text the mod stored.
  const description =
    entry?.translations?.[i18n.language]?.description ?? mod?.description ?? entry?.description;
  const links = modLinks(t, mod, entry);
  const added = mod ? mod.addedAt : undefined;

  const stats: [string, ReactNode][] = [
    [t("profile.playTime"), formatPlayTime(t, profile.playSeconds)],
    [t("profile.sessions"), String(profile.sessionCount)],
    [
      t("library.lastPlayed"),
      profile.lastPlayedAt === null ? "–" : <RelativeTime at={profile.lastPlayedAt} />,
    ],
    [t("library.saveCountLabel"), String(profile.saveCount)],
  ];
  if (added !== undefined) stats.push([t("profile.added"), formatDate(i18n.language, added)]);

  const actions: MenuItem[] = [];
  if (mod && installs.data && installs.data.length > 1)
    actions.push({
      label: t("profile.playOn"),
      icon: <ArrowLeftRight className="size-4" />,
      onSelect: () => setPicking(true),
      disabled: profile.running,
    });
  if (mod) {
    actions.push({
      label: t("profile.openModFolder"),
      icon: <FolderOpen className="size-4" />,
      onSelect: () => openFolder.mutate(mod.id),
    });
    if (checkable)
      actions.push({
        label: t("modUpdate.check"),
        icon: <RefreshCw className="size-4" />,
        onSelect: () => check.mutate(),
        disabled: check.isPending,
      });
    actions.push({
      label: t("profile.replaceMod"),
      icon: <FileArchive className="size-4" />,
      onSelect: () => setReplacing(true),
      disabled: profile.running,
    });
  }
  actions.push({
    label: mod ? t("mods.remove") : t("profile.removeGame"),
    icon: <Trash2 className="size-4" />,
    onSelect: () => setRemoving(true),
    disabled: profile.running,
    danger: true,
    separated: actions.length > 0,
  });

  return (
    <Card ref={cardRef} className="mb-6 flex flex-col md:flex-row">
      {/* As tall as the text, at 4:3. Where the width limits get in the way,
          pictures keep 4:3 centred on the card's own colour, not on black
          bars like a misfit video; only the placeholder fills the box. */}
      <div className="flex shrink-0 items-center" style={{ width: pictureWidth }}>
        {cover.isPending ? (
          // The title screen is read from the files on first use; empty until
          // then, so the placeholder doesn't flash up before it.
          <div className="aspect-[4/3] w-full" />
        ) : cover.data || !latest ? (
          <Cover
            gameId={profile.gameId}
            game={game}
            src={cover.data?.src}
            className={`aspect-[4/3] w-full text-7xl ${cover.data ? "" : "h-full"}`}
          />
        ) : (
          <img
            src={screenshotSrc(latest)}
            alt={t("screenshots.alt", { game: profile.label })}
            className="aspect-[4/3] w-full object-cover [image-rendering:pixelated]"
          />
        )}
      </div>
      {/* About it on top, playing it at the bottom. */}
      <div ref={textRef} className="flex min-w-0 flex-1 flex-col justify-between gap-5 p-5">
        <div className="grid content-start gap-4 select-text">
          <div className="grid gap-2">
            <h1 className="font-pixel text-lg leading-relaxed">{profile.label}</h1>
            <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
              {profile.missingGame !== null && (
                <span>{t("mods.waitsFor", { game: profile.missingGame })}</span>
              )}
              {profile.modId !== null && profile.missingGame === null && game && (
                <span>
                  <RunsOn
                    profile={profile}
                    game={game.title}
                    version={version?.label ?? t("library.unknownVersion")}
                  />
                </span>
              )}
              {profile.modId !== null && <Badge>{t("mods.badge")}</Badge>}
              {profile.modId === null && (
                <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
              )}
              {profile.modId === null && version?.shareware && (
                <Badge>{t("profile.shareware")}</Badge>
              )}
              {mod?.version && (
                // "v2.0" like the game versions; versions with words stay as written.
                <Badge>{/^\d/.test(mod.version) ? `v${mod.version}` : mod.version}</Badge>
              )}
            </div>
            {mod && mod.authors.length > 0 && (
              <p className="text-sm">
                <Authors
                  authors={mod.authors}
                  links={{ ...mod.authorLinks, ...entry?.authorLinks }}
                />
              </p>
            )}
            {mod?.released && (
              <p className="text-sm text-muted">
                {t("profile.released", { date: formatLooseDate(i18n.language, mod.released) })}
              </p>
            )}
            {extensions.length > 0 && (
              <p className="text-sm">
                <span className="text-muted">
                  {t("profile.playedOverIt", { count: extensions.length })}
                </span>{" "}
                {extensions.map((m, i) => (
                  <Fragment key={m.id}>
                    {i > 0 && ", "}
                    <Link
                      to="/profiles/$profileId"
                      params={{ profileId: m.profileId }}
                      className="underline-offset-2 hover:text-accent hover:underline"
                    >
                      {m.name}
                    </Link>
                  </Fragment>
                ))}
              </p>
            )}
            {links.length > 0 && (
              <ul
                className="flex flex-wrap gap-x-4 gap-y-1 text-sm"
                aria-label={t("profile.links")}
              >
                {links.map(([label, url]) => (
                  <li key={url}>
                    <button
                      type="button"
                      onClick={() => openUrl(url)}
                      className="inline-flex items-center gap-1 text-muted underline-offset-2 hover:text-accent hover:underline"
                    >
                      <ExternalLink className="size-3.5" aria-hidden />
                      {label}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
          {description && (
            <p className="max-w-prose text-sm leading-relaxed whitespace-pre-line">{description}</p>
          )}
          {cover.data?.scrambled && (
            <p className="flex max-w-prose items-start gap-2 text-sm">
              <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
              {t("profile.scrambled")}
            </p>
          )}
          {profile.sessionCount === 0 ? (
            // Nothing to count yet: one line instead of columns of "0" and "–".
            // It wraps only between the parts, never inside a date.
            <p className="text-sm text-muted">
              {[
                t("library.neverPlayed"),
                t("library.saveCount", { count: profile.saveCount }),
                added !== undefined && `${t("profile.added")} ${formatDate(i18n.language, added)}`,
              ]
                .filter(Boolean)
                .map((part, i) => (
                  <Fragment key={i}>
                    {i > 0 && " · "}
                    <span className="whitespace-nowrap">{part}</span>
                  </Fragment>
                ))}
            </p>
          ) : (
            // Columns, so the second row lines up with the first.
            <dl className="grid grid-cols-[repeat(auto-fill,minmax(5.5rem,1fr))] gap-x-4 gap-y-2">
              {stats.map(([label, value]) => (
                <div key={label} className="grid gap-1">
                  <dt className="text-xs text-muted">{label}</dt>
                  <dd className="text-sm font-semibold">{value}</dd>
                </div>
              ))}
            </dl>
          )}
        </div>
        <div className="grid gap-3">
          <div className="flex items-stretch gap-2">
            <PlayButton
              onClick={onPlay}
              disabled={!canPlay || profile.running || profile.missingGame !== null}
              className="min-w-44 px-8 py-3 text-base"
            >
              {profile.missingGame !== null
                ? t("mods.gameMissing")
                : profile.running
                  ? t("library.running")
                  : t("library.play")}
            </PlayButton>
            <Menu
              label={t("profile.more")}
              trigger={
                <>
                  {t("profile.more")}
                  <ChevronDown className="size-4" aria-hidden />
                </>
              }
              className="px-4 text-base"
              items={actions}
            />
          </div>
          {profile.missingGame !== null && (
            <MissingGame profile={profile} missingGame={profile.missingGame} game={game} />
          )}
          {checkable && <ModUpdate mod={mod} check={check} disabled={profile.running} />}
          {mod?.source === "keenwiki" && !mod.indexId && (
            <p className="text-sm">
              <SuggestToIndex modId={mod.id} />
            </p>
          )}
          <FiringNote profileId={profile.id} />
          <MachineNote profileId={profile.id} />
          {openFolder.error && <Notice tone="error">{errorMessage(t, openFolder.error)}</Notice>}
        </div>
      </div>
      {removing && profile.modId === null && profile.installId !== null && (
        <RemoveGameDialog
          profile={profile}
          installId={profile.installId}
          modCount={modCount}
          onClose={() => setRemoving(false)}
          onRemoved={() => {
            forgetRemovedCovers(queryClient);
            queryClient.invalidateQueries({ queryKey: modsKey });
            queryClient.invalidateQueries({ queryKey: installsKey });
            navigate({ to: "/" });
          }}
        />
      )}
      {removing && profile.modId !== null && (
        <RemoveModDialog
          profile={{ ...profile, modId: profile.modId }}
          onClose={() => setRemoving(false)}
          onRemoved={() => {
            // With the mods played over it, removed with it.
            forgetRemovedCovers(queryClient);
            queryClient.invalidateQueries({ queryKey: modsKey });
            navigate({ to: "/" });
          }}
        />
      )}
      {replacing && mod && (
        <AddModDialog
          replacing={mod.id}
          onClose={() => setReplacing(false)}
          onInstalled={() => {
            setReplacing(false);
            queryClient.invalidateQueries({ queryKey: profilesKey });
          }}
        />
      )}
      {picking && mod && profile.installId !== null && installs.data && (
        <GamePicker
          modId={mod.id}
          installId={profile.installId}
          installs={installs.data}
          game={game}
          disabled={profile.running}
          onClose={() => setPicking(false)}
        />
      )}
    </Card>
  );
}

/** What a mod waits for, and where to get it: free shareware from Discover, else the add dialog. */
function MissingGame({
  profile,
  missingGame,
  game,
}: {
  profile: Profile;
  missingGame: string;
  game?: Game;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const free = useFreeGames();
  const [adding, setAdding] = useState(false);
  const [finding, setFinding] = useState(false);
  // A full shareware episode, not a demo or beta.
  const shareware = (free.data ?? []).some(
    (f) =>
      f.gameId === profile.gameId &&
      (game?.versions.find((v) => v.id === f.versionId)?.kind ?? "release") === "release",
  );
  return (
    <div className="grid justify-items-start gap-2">
      <Notice>{t("mods.waitingHint", { game: missingGame })}</Notice>
      {shareware ? (
        <Link to="/discover" search={{ tab: "games" }} className={buttonClass("secondary")}>
          <Download className="size-4" aria-hidden />
          {t("mods.getFreeGame")}
        </Link>
      ) : (
        <Button variant="secondary" onClick={() => setAdding(true)}>
          <PackagePlus className="size-4" aria-hidden />
          {t("mods.addGame")}
        </Button>
      )}
      {adding && (
        <AddModDialog
          onClose={() => setAdding(false)}
          onInstalled={(mod) => {
            setAdding(false);
            queryClient.invalidateQueries({ queryKey: profilesKey });
            navigate({ to: "/profiles/$profileId", params: { profileId: mod.profileId } });
          }}
          // The mod's profile takes the game as soon as the list is fetched again.
          onGamesImported={() => setAdding(false)}
          onFindOwned={() => {
            setAdding(false);
            setFinding(true);
          }}
        />
      )}
      {finding && (
        <FindOwnedDialog onClose={() => setFinding(false)} onImported={() => setFinding(false)} />
      )}
    </div>
  );
}

/** Which copy of the game a mod plays on, when more than one fits. */
function GamePicker({
  modId,
  installId,
  installs,
  game,
  disabled,
  onClose,
}: {
  modId: string;
  installId: string;
  installs: Install[];
  game?: Game;
  disabled: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const change = useMutation({
    mutationFn: (id: string) => unwrap(commands.setModInstall(modId, id)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: profileFiringKey(modId) });
    },
  });
  const versionLabel = (versionId: string | null) =>
    game?.versions.find((v) => v.id === versionId)?.label ?? t("library.unknownVersion");

  return (
    <Dialog title={t("mods.playOn")} onClose={onClose}>
      <select
        aria-label={t("mods.playOn")}
        data-autofocus
        className="min-w-0 truncate rounded-lg border border-border bg-surface-raised px-2 py-1.5 text-sm"
        value={installId}
        disabled={disabled || change.isPending}
        onChange={(e) => change.mutate(e.target.value)}
      >
        {installs.map((i) => (
          <option key={i.id} value={i.id}>
            {game?.shortTitle ?? i.label} ({versionLabel(i.versionId)})
          </option>
        ))}
      </select>
      <p className="text-sm text-muted">{t("mods.changeGameHint")}</p>
      {change.error && <Notice tone="error">{errorMessage(t, change.error)}</Notice>}
      <div className="flex justify-end">
        <Button variant="secondary" onClick={onClose}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}

/** How Keen 1-3 shoot here, where it isn't the usual Ctrl and Alt or can't be changed. */
function FiringNote({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const enabled = useQuery({
    queryKey: oneButtonFiringKey,
    queryFn: () => unwrap(commands.oneButtonFiring()),
  });
  const status = useQuery({
    queryKey: profileFiringKey(profileId),
    queryFn: () => unwrap(commands.profileFiring(profileId)),
  });
  const s = status.data;
  // A mod's own one-button firing is worth knowing even with the setting off.
  if (!s || (s !== "builtIn" && !enabled.data)) return null;
  return (
    <p className="flex items-start gap-1.5 text-sm text-muted">
      <Keyboard className="mt-0.5 size-4 shrink-0" aria-hidden />
      {t(`profile.firing.${s === "patchable" ? "on" : s}`)}
    </p>
  );
}

/** The machine DOSBox runs as for a mod that needs another than the usual one. */
function MachineNote({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const machine = useQuery({
    queryKey: ["machine", profileId],
    queryFn: () => unwrap(commands.profileMachine(profileId)),
  });
  if (!machine.data) return null;
  return (
    <p className="flex items-start gap-1.5 text-sm text-muted">
      <MonitorCog className="mt-0.5 size-4 shrink-0" aria-hidden />
      {t(`profile.machine.${machine.data}`)}
    </p>
  );
}

function modLinks(
  t: (key: string) => string,
  mod: Mod | undefined,
  entry: IndexEntry | undefined,
): [string, string][] {
  if (!mod) return [];
  const all: [string, string | null | undefined][] = [
    ["KeenWiki", mod.keenwikiUrl ?? entry?.links.keenwiki],
    ["PCKF", mod.pckf ?? entry?.links.pckf],
    ["Keen: Modding", mod.keenmodding ?? entry?.links.keenmodding],
    [t("discover.homepage"), mod.homepage ?? entry?.links.homepage],
  ];
  return all.filter((l): l is [string, string] => !!l[1]);
}

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link to="/" className="mb-2 inline-flex items-center gap-1 text-sm text-muted hover:text-ink">
      <ArrowLeft className="size-4" aria-hidden />
      {t("profile.back")}
    </Link>
  );
}
