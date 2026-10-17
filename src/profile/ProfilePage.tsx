import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowLeft, ExternalLink } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type IndexEntry, type Mod, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { useIndexMods } from "../discover/queries";
import { errorMessage } from "../errors";
import { formatDate, formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { Cover } from "../library/Cover";
import { modsKey, profilesKey } from "../library/queries";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { PlayButton } from "../ui/PlayButton";
import { Tabs } from "../ui/Tabs";
import { ConfigPanel } from "./ConfigPanel";
import { SavesPanel } from "./SavesPanel";
import { ScreenshotsPanel } from "./ScreenshotsPanel";
import { screenshotSrc, screenshotsKey, useScreenshots } from "./screenshots";
import { backupsKey, savesKey } from "./saves";

type Tab = "saves" | "screenshots" | "config";

export function ProfileRoute() {
  const { profileId } = useParams({ from: "/profiles/$profileId" });
  return <ProfileDetail profileId={profileId} />;
}

/** One game or mod: play it, read about it, manage saves, screenshots and settings (§8.3). */
export function ProfileDetail({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const runtime = useRuntimeStatus();
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });
  const [tab, setTab] = useState<Tab>("saves");

  useEffect(() => {
    // The game may have saved, made an automatic backup before, or the
    // player took screenshots.
    const unlisten = events.playSessionEnded.listen(() => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: savesKey(profileId) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profileId) });
      queryClient.invalidateQueries({ queryKey: screenshotsKey(profileId) });
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

  if (profiles.data && !profile) {
    return (
      <>
        <BackLink />
        <Notice tone="error">{t("profile.notFound")}</Notice>
      </>
    );
  }

  const tabs: { id: Tab; label: string }[] = [
    { id: "saves", label: t("profile.tabSaves") },
    { id: "screenshots", label: t("profile.tabScreenshots") },
    { id: "config", label: t("profile.tabConfig") },
  ];

  return (
    <>
      <BackLink />
      {profile && (
        <Hero
          profile={profile}
          game={game}
          canPlay={!!runtime.data?.active && !launch.isPending}
          onPlay={() => launch.mutate()}
        />
      )}
      {launch.error && (
        <div className="mb-4">
          <Notice tone="error">{errorMessage(t, launch.error)}</Notice>
        </div>
      )}

      <Tabs tabs={tabs} selected={tab} onSelect={setTab} label={t("profile.tabs")}>
        <div className={tab === "screenshots" ? "" : "max-w-3xl"}>
          {profile && tab === "saves" && <SavesPanel profile={profile} game={game} />}
          {profile && tab === "screenshots" && <ScreenshotsPanel profile={profile} />}
          {profile && tab === "config" && <ConfigPanel profileId={profile.id} />}
        </div>
      </Tabs>
    </>
  );
}

/** Picture, title, credits, description, links and stats. */
function Hero({
  profile,
  game,
  canPlay,
  onPlay,
}: {
  profile: Profile;
  game?: Game;
  canPlay: boolean;
  onPlay: () => void;
}) {
  const { t, i18n } = useTranslation();
  const mods = useQuery({
    queryKey: modsKey,
    queryFn: () => unwrap(commands.listMods()),
    enabled: profile.modId !== null,
  });
  const index = useIndexMods();
  const shots = useScreenshots(profile.id);

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
    <Card className="mb-6 grid md:grid-cols-[minmax(0,20rem)_1fr]">
      <div className="bg-black">
        {latest ? (
          <img
            src={screenshotSrc(latest)}
            alt={t("screenshots.alt", { game: profile.label })}
            className="aspect-[4/3] h-full w-full object-cover [image-rendering:pixelated]"
          />
        ) : (
          <Cover
            gameId={profile.gameId}
            game={game}
            className="aspect-[4/3] h-full w-full text-7xl"
          />
        )}
      </div>
      <div className="grid content-start gap-4 p-5">
        <div className="grid gap-2">
          <h1 className="font-pixel text-lg leading-relaxed">{profile.label}</h1>
          <div className="flex flex-wrap items-center gap-2 text-sm text-muted">
            {profile.modId !== null && game && <span>{game.title}</span>}
            {profile.modId !== null && <Badge>{t("mods.badge")}</Badge>}
            <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
            {version?.shareware && <Badge>{t("profile.shareware")}</Badge>}
            {mod?.version && <Badge>{t("mods.version", { version: mod.version })}</Badge>}
          </div>
          {mod && mod.authors.length > 0 && (
            <p className="text-sm">{t("mods.by", { authors: mod.authors.join(", ") })}</p>
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
        <div>
          <PlayButton
            onClick={onPlay}
            disabled={!canPlay || profile.running}
            className="px-6 py-3 text-base"
          >
            {profile.running ? t("library.running") : t("library.play")}
          </PlayButton>
        </div>
      </div>
    </Card>
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
