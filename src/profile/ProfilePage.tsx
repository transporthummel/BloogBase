import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link, useParams, useSearch } from "@tanstack/react-router";
import { ArrowLeft, Info } from "lucide-react";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type WorldStart } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { scroller } from "../library/scroll";
import { profilesKey, useProfiles } from "../library/queries";
import { useRuntimeStatus } from "../runtime/queries";
import { Notice } from "../ui/Notice";
import { Tabs, type TabItem } from "../ui/Tabs";
import { ConfigPanel } from "./ConfigPanel";
import { ControlsSheet } from "./Controls";
import { useControlsGate } from "./controlsGate";
import { Hero } from "./Hero";
import { ReadmePanel } from "./ReadmePanel";
import { useReadmes } from "./readme";
import { SavesPanel } from "./SavesPanel";
import { ScreenshotsPanel } from "./ScreenshotsPanel";
import { VideosPanel } from "./VideosPanel";
import { WorldsPanel } from "./WorldsPanel";
import { useWorlds, worldsKey } from "./worlds";
import { useVariants, variantsKey } from "./variants";
import { useVariantGate } from "./variantGate";
import { VariantsPanel } from "./VariantsPanel";
import { screenshotsKey, useScreenshots, useVideos, videosKey } from "./captures";
import { backupsKey, savesKey, useSaves } from "./saves";

type Tab =
  "readme" | "controls" | "worlds" | "variants" | "saves" | "screenshots" | "videos" | "config";

export function ProfileRoute() {
  const { profileId } = useParams({ from: "/profiles/$profileId" });
  const { show } = useSearch({ from: "/profiles/$profileId" });
  // Keyed, so going from one profile straight to another (a mod played over
  // another links to it) starts on its first tab again.
  return <ProfileDetail key={profileId} profileId={profileId} show={show} />;
}

/** One game or mod: play it, read about it, manage saves, screenshots, videos and settings (§8.3). */
export function ProfileDetail({
  profileId,
  show,
}: {
  profileId: string;
  /**
   * The library card's links: `variant` opens on the Variants tab, at the
   * variant played; `saves` on the Saves tab.
   */
  show?: "variant" | "saves";
}) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const games = useGames();
  const runtime = useRuntimeStatus();
  const profiles = useProfiles();
  // Until the player picks one: the README if the mod has one, else saves.
  const [picked, setTab] = useState<Tab | null>(
    show === "variant" ? "variants" : show === "saves" ? "saves" : null,
  );
  // The Variants tab scrolls to the variant played, once, when asked to.
  const [seekVariant, setSeekVariant] = useState(show === "variant");

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
      queryClient.invalidateQueries({ queryKey: worldsKey(profileId) });
      queryClient.invalidateQueries({ queryKey: variantsKey(profileId) });
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [queryClient, profileId]);

  const launch = useMutation({
    mutationFn: ({ world, variant }: { world: WorldStart | null; variant: string | null }) =>
      unwrap(commands.launchProfile(profileId, world, variant, i18n.language)),
    onSettled: () => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: backupsKey(profileId) });
      // Starting a world swaps the saves the game sees.
      queryClient.invalidateQueries({ queryKey: savesKey(profileId) });
      queryClient.invalidateQueries({ queryKey: worldsKey(profileId) });
      // Starting another variant swaps them too.
      queryClient.invalidateQueries({ queryKey: variantsKey(profileId) });
    },
  });
  // The world and variant to start once the controls have been shown, if they are.
  const world = useRef<WorldStart | null>(null);
  const variant = useRef<string | null>(null);

  const profile = profiles.data?.find((p) => p.id === profileId);
  const game = games.data?.find((g) => g.id === profile?.gameId);
  // The first start of a series shows its keys first.
  const gate = useControlsGate(() =>
    launch.mutate({ world: world.current, variant: variant.current }),
  );
  // A mod with variants asks which one until the player has chosen (§9.7).
  const variantGate = useVariantGate((p, picked) => {
    variant.current = picked;
    gate.play(p, game);
  });
  const readmes = useReadmes(profile);
  const saves = useSaves(profileId);
  const shots = useScreenshots(profileId);
  const videos = useVideos(profileId);
  const worlds = useWorlds({ id: profileId, worlds: !!profile?.worlds });
  const variants = useVariants({ id: profileId, variantCount: profile?.variantCount ?? 0 });

  /** With `pick`, that variant; without, the one chosen, or the picker. */
  function play(start: WorldStart | null, pick?: string) {
    if (!profile) return;
    world.current = start;
    variantGate.play(profile, pick);
  }

  // A mod just installed opens before the list has been fetched again.
  if (profiles.data && !profile && !profiles.isFetching) {
    return (
      <>
        <BackLink />
        <Notice tone="error">{t("profile.notFound")}</Notice>
      </>
    );
  }

  const readme = !!readmes.data?.length;
  const gameKeys = !!game && !profile?.modId;
  const controls = { id: "controls" as const, label: t("profile.tabControls") };
  const tabs: TabItem<Tab>[] = [
    // What to play comes first for a mod with worlds or variants.
    ...(profile?.worlds
      ? [
          {
            id: "worlds" as const,
            label: t("profile.tabWorlds"),
            count: worlds.data?.worlds?.length,
          },
        ]
      : []),
    ...(profile && profile.variantCount > 0
      ? [
          {
            id: "variants" as const,
            label: t("variants.tab"),
            count: variants.data?.variants?.length,
          },
        ]
      : []),
    ...(readme ? [{ id: "readme" as const, label: t("profile.tabReadme") }] : []),
    // A game's keys are what its page is read for, after its README; without
    // one after the saves, so the page opens on its first tab. A mod's own
    // README comes first.
    ...(gameKeys && readme ? [controls] : []),
    { id: "saves", label: t("profile.tabSaves"), count: saves.data?.length },
    ...(gameKeys && !readme ? [controls] : []),
    {
      id: "screenshots",
      label: t("profile.tabScreenshots"),
      count: shots.data?.length,
    },
    { id: "videos", label: t("profile.tabVideos"), count: videos.data?.length },
    ...(game && profile?.modId ? [controls] : []),
    { id: "config", label: t("profile.tabConfig") },
  ];
  // Opens on the first: the worlds or variants, else the README, else the saves.
  const tab = tabs.find((x) => x.id === picked)?.id ?? tabs[0].id;
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
          onPlay={play}
          onShowVariant={() => {
            setTab("variants");
            setSeekVariant(true);
          }}
          modCount={
            profiles.data?.filter((p) => p.modId !== null && p.installId === profile.installId)
              .length ?? 0
          }
        />
      )}
      {gate.dialog}
      {variantGate.dialog}
      {launch.error && (
        <div className="mb-4">
          <Notice tone="error">{errorMessage(t, launch.error)}</Notice>
        </div>
      )}

      {profile && tabsReady && (
        <Tabs tabs={tabs} selected={tab} onSelect={setTab} label={t("profile.tabs")}>
          {/* Text and lists of rows stay narrow to be read; keys, variants,
              worlds and captures take the width beside the picture above. */}
          <div
            className={`@container ${["readme", "saves", "config"].includes(tab) ? "max-w-3xl" : ""}`}
          >
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
            {tab === "worlds" && (
              <WorldsPanel
                profile={profile}
                canPlay={!!runtime.data?.active && !launch.isPending}
                onPlay={play}
                onShowSaves={() => setTab("saves")}
              />
            )}
            {tab === "variants" && (
              <VariantsPanel
                profile={profile}
                canPlay={!!runtime.data?.active && !launch.isPending}
                onPlay={(v) => play(null, v)}
                seekActive={seekVariant}
                onSought={() => setSeekVariant(false)}
                onShowSaves={() => setTab("saves")}
              />
            )}
            {tab === "saves" && (
              <SavesPanel
                profile={profile}
                game={game}
                onShowWorlds={() => setTab("worlds")}
                onShowVariants={() => setTab("variants")}
              />
            )}
            {tab === "screenshots" && <ScreenshotsPanel profile={profile} />}
            {tab === "videos" && <VideosPanel profile={profile} />}
            {tab === "config" && <ConfigPanel profileId={profile.id} />}
          </div>
        </Tabs>
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
