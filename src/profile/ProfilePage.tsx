import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { ArrowLeft, Info } from "lucide-react";
import { useEffect, useLayoutEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, events } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { scroller } from "../library/scroll";
import { profilesKey } from "../library/queries";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Notice } from "../ui/Notice";
import { Tabs } from "../ui/Tabs";
import { ConfigPanel } from "./ConfigPanel";
import { ControlsSheet } from "./Controls";
import { useControlsGate } from "./controlsGate";
import { Hero } from "./Hero";
import { ReadmePanel } from "./ReadmePanel";
import { useReadmes } from "./readme";
import { SavesPanel } from "./SavesPanel";
import { ScreenshotsPanel } from "./ScreenshotsPanel";
import { VideosPanel } from "./VideosPanel";
import { screenshotsKey, useScreenshots, useVideos, videosKey } from "./captures";
import { backupsKey, savesKey, useSaves } from "./saves";

type Tab = "readme" | "controls" | "saves" | "screenshots" | "videos" | "config";

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
  // The first start of a series shows its keys first.
  const gate = useControlsGate(() => launch.mutate());
  const readmes = useReadmes(profile);
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
    // A game's keys are what its page is read for; a mod's own README comes first.
    ...(game && !profile?.modId
      ? [{ id: "controls" as const, label: t("profile.tabControls") }]
      : []),
    { id: "saves", label: <Counted label={t("profile.tabSaves")} items={saves.data} /> },
    {
      id: "screenshots",
      label: <Counted label={t("profile.tabScreenshots")} items={shots.data} />,
    },
    { id: "videos", label: <Counted label={t("profile.tabVideos")} items={videos.data} /> },
    ...(game && profile?.modId
      ? [{ id: "controls" as const, label: t("profile.tabControls") }]
      : []),
    { id: "config", label: t("profile.tabConfig") },
  ];
  // Opens on the README, else on the saves, as before there was a Controls tab.
  const tab =
    tabs.find((x) => x.id === picked)?.id ?? (tabs[0].id === "readme" ? "readme" : "saves");
  // Wait for the README so the first tab doesn't change under the player.
  const tabsReady = !(readmes.isEnabled && readmes.isPending);

  return (
    <>
      <BackLink />
      {profile && (
        <Hero
          profile={profile}
          game={game}
          canPlay={!!runtime.data?.active && !launch.isPending}
          onPlay={() => gate.play(profile, game)}
          modCount={
            profiles.data?.filter((p) => p.modId !== null && p.installId === profile.installId)
              .length ?? 0
          }
        />
      )}
      {gate.dialog}
      {launch.error && (
        <div className="mb-4">
          <Notice tone="error">{errorMessage(t, launch.error)}</Notice>
        </div>
      )}

      {profile && tabsReady && (
        <Tabs tabs={tabs} selected={tab} onSelect={setTab} label={t("profile.tabs")}>
          <div className={tab === "screenshots" || tab === "videos" ? "" : "max-w-3xl"}>
            {readmes.data && tab === "readme" && (
              <>
                {/* Readmes tell players of their time to run batch files and set up DOSBox. */}
                <p className="mb-3 flex items-start gap-2 text-sm text-muted">
                  <Info className="mt-0.5 size-4 shrink-0" aria-hidden />
                  {t(profile.modId ? "readme.handled" : "readme.handledGame")}
                </p>
                {profile.modId ? (
                  <ReadmePanel key={profile.id} modId={profile.modId} readmes={readmes.data} />
                ) : (
                  profile.installId && (
                    <ReadmePanel
                      key={profile.id}
                      installId={profile.installId}
                      readmes={readmes.data}
                    />
                  )
                )}
              </>
            )}
            {tab === "controls" && game && <ControlsSheet profile={profile} game={game} />}
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

function BackLink() {
  const { t } = useTranslation();
  return (
    <Link to="/" className="mb-2 inline-flex items-center gap-1 text-sm text-muted hover:text-ink">
      <ArrowLeft className="size-4" aria-hidden />
      {t("profile.back")}
    </Link>
  );
}
