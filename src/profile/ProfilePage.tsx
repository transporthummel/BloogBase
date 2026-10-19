import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, ExternalLink, FolderOpen, Trash2 } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type IndexEntry, type Mod, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { formatDate, formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { installsKey, modsKey, profilesKey } from "../library/queries";
import { Authors } from "../mods/Authors";
import { RemoveModDialog } from "../mods/RemoveModDialog";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { PlayButton } from "../ui/PlayButton";
import { Tabs } from "../ui/Tabs";
import { ConfigPanel } from "./ConfigPanel";
import { oneButtonFiringKey, profileFiringKey } from "./firing";
import { ReadmePanel } from "./ReadmePanel";
import { RemoveGameDialog } from "./RemoveGameDialog";
import { useReadme } from "./readme";
import { SavesPanel } from "./SavesPanel";
import { ScreenshotsPanel } from "./ScreenshotsPanel";
import { VideosPanel } from "./VideosPanel";
import { screenshotSrc, screenshotsKey, useScreenshots, videosKey } from "./captures";
import { usePictureWidth } from "./heroPicture";
import { backupsKey, savesKey } from "./saves";

type Tab = "readme" | "saves" | "screenshots" | "videos" | "config";

export function ProfileRoute() {
  const { profileId } = useParams({ from: "/profiles/$profileId" });
  return <ProfileDetail profileId={profileId} />;
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
  const readme = useReadme(profile?.modId ?? null);

  if (profiles.data && !profile) {
    return (
      <>
        <BackLink />
        <Notice tone="error">{t("profile.notFound")}</Notice>
      </>
    );
  }

  const tabs: { id: Tab; label: string }[] = [
    ...(readme.data ? [{ id: "readme" as const, label: t("profile.tabReadme") }] : []),
    { id: "saves", label: t("profile.tabSaves") },
    { id: "screenshots", label: t("profile.tabScreenshots") },
    { id: "videos", label: t("profile.tabVideos") },
    { id: "config", label: t("profile.tabConfig") },
  ];
  const tab = tabs.find((x) => x.id === picked)?.id ?? tabs[0].id;
  // Wait for the README so the first tab doesn't change under the player.
  const tabsReady = !(profile?.modId && readme.isPending);

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
            {readme.data && tab === "readme" && <ReadmePanel readme={readme.data} />}
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

/** Picture, title, credits, description, links and stats. */
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
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  const mod = mods.data?.find((m) => m.id === profile.modId);
  const entry = mod?.indexId
    ? index.data?.find((x) => x.entry.id === mod.indexId)?.entry
    : undefined;
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const latest = shots.data?.[0];
  const description = mod?.description ?? entry?.description;
  const links = modLinks(t, mod, entry);
  const added = mod ? mod.addedAt : undefined;

  const stats: [string, string][] = [
    [
      t("profile.playTime"),
      profile.sessionCount === 0
        ? t("library.neverPlayed")
        : formatPlayTime(t, profile.playSeconds),
    ],
    [t("profile.sessions"), String(profile.sessionCount)],
    [
      t("library.lastPlayed"),
      profile.lastPlayedAt === null ? "–" : formatDate(i18n.language, profile.lastPlayedAt),
    ],
    [t("library.saveCountLabel"), String(profile.saveCount)],
  ];
  if (added !== undefined) stats.push([t("profile.added"), formatDate(i18n.language, added)]);

  return (
    <Card ref={cardRef} className="mb-6 flex flex-col md:flex-row">
      {/* As tall as the text, at 4:3. Where the width limits get in the way,
          pictures keep 4:3 on black; only the placeholder fills the box. */}
      <div className="flex shrink-0 items-center bg-black" style={{ width: pictureWidth }}>
        {cover.data || cover.isPending || !latest ? (
          <Cover
            gameId={profile.gameId}
            game={game}
            src={cover.data}
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
      <div ref={textRef} className="grid min-w-0 flex-1 content-start gap-4 p-5">
        <div className="grid gap-2">
          <h1 className="font-pixel text-lg leading-relaxed">{profile.label}</h1>
          <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
            {profile.missingGame !== null && (
              <span>{t("mods.waitsFor", { game: profile.missingGame })}</span>
            )}
            {profile.modId !== null && profile.missingGame === null && game && (
              <span>
                {t("mods.baseGame", {
                  game: game.title,
                  version: version?.label ?? t("library.unknownVersion"),
                })}
              </span>
            )}
            {profile.modId !== null && <Badge>{t("mods.badge")}</Badge>}
            {profile.modId === null && (
              <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
            )}
            {profile.modId === null && version?.shareware && (
              <Badge>{t("profile.shareware")}</Badge>
            )}
            {mod?.version && <Badge>{t("mods.version", { version: mod.version })}</Badge>}
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
            <p className="text-sm text-muted">{t("profile.released", { date: mod.released })}</p>
          )}
        </div>
        {description && (
          <p className="max-w-prose text-sm leading-relaxed whitespace-pre-line">{description}</p>
        )}
        {links.length > 0 && (
          <ul className="flex flex-wrap gap-2" aria-label={t("profile.links")}>
            {links.map(([label, url]) => (
              <li key={url}>
                <Button variant="secondary" className="px-3 py-1.5" onClick={() => openUrl(url)}>
                  <ExternalLink className="size-4" aria-hidden />
                  {label}
                </Button>
              </li>
            ))}
          </ul>
        )}
        <dl className="flex flex-wrap gap-x-6 gap-y-2">
          {stats.map(([label, value]) => (
            <div key={label} className="grid gap-1">
              <dt className="text-xs text-muted">{label}</dt>
              <dd className="text-sm font-semibold">{value}</dd>
            </div>
          ))}
        </dl>
        <div className="flex flex-wrap items-center gap-3">
          <PlayButton
            onClick={onPlay}
            disabled={!canPlay || profile.running || profile.missingGame !== null}
            className="px-6 py-3 text-base"
          >
            {profile.missingGame !== null
              ? t("mods.gameMissing")
              : profile.running
                ? t("library.running")
                : t("library.play")}
          </PlayButton>
          {mod && (
            <>
              <Button variant="secondary" onClick={() => openFolder.mutate(mod.id)}>
                <FolderOpen className="size-4" aria-hidden />
                {t("profile.openModFolder")}
              </Button>
              <Button
                variant="ghost"
                onClick={() => setRemoving(true)}
                disabled={profile.running}
                aria-label={t("mods.removeNamed", { name: profile.label })}
              >
                <Trash2 className="size-4" aria-hidden />
                {t("mods.remove")}
              </Button>
            </>
          )}
          {profile.modId === null && (
            <Button
              variant="ghost"
              onClick={() => setRemoving(true)}
              disabled={profile.running}
              aria-label={t("profile.removeGameNamed", { name: profile.label })}
            >
              <Trash2 className="size-4" aria-hidden />
              {t("profile.removeGame")}
            </Button>
          )}
        </div>
        {profile.missingGame !== null && (
          <Notice>{t("mods.waitingHint", { game: profile.missingGame })}</Notice>
        )}
        {removing && profile.modId === null && profile.installId !== null && (
          <RemoveGameDialog
            profile={profile}
            installId={profile.installId}
            modCount={modCount}
            onClose={() => setRemoving(false)}
            onRemoved={() => {
              queryClient.invalidateQueries({ queryKey: profilesKey });
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
              queryClient.invalidateQueries({ queryKey: profilesKey });
              queryClient.invalidateQueries({ queryKey: modsKey });
              navigate({ to: "/" });
            }}
          />
        )}
        <FiringNote profileId={profile.id} />
        {openFolder.error && <Notice tone="error">{errorMessage(t, openFolder.error)}</Notice>}
      </div>
    </Card>
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
    <p className="text-sm text-muted">{t(`profile.firing.${s === "patchable" ? "on" : s}`)}</p>
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
    ["PCKF", entry?.links.pckf],
    [t("discover.homepage"), entry?.links.homepage],
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
