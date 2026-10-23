import { useQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Profile } from "../bindings";
import { useGames } from "../catalog";
import { unwrap } from "../ipc";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { profilesKey } from "../library/queries";
import { Bloog } from "../ui/Bloog";

/** Should DOSBox never say its window is up, the overlay goes anyway. */
const GIVE_UP_MS = 15_000;

/**
 * Covers the seconds between Play and DOSBox's window (mostly its graphics
 * setup) with the game's title screen. A click takes it away early.
 */
export function LaunchOverlay() {
  const [starting, setStarting] = useState<string | null>(null);
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });

  useEffect(() => {
    const done = (id: string) => setStarting((s) => (s === id ? null : s));
    const launches = events.gameLaunch.listen(({ payload }) => {
      if (payload.phase === "starting") setStarting(payload.profileId);
      else done(payload.profileId);
    });
    const ends = events.playSessionEnded.listen(({ payload }) => done(payload.profileId));
    return () => {
      launches.then((f) => f());
      ends.then((f) => f());
    };
  }, []);

  useEffect(() => {
    if (starting === null) return;
    const timer = setTimeout(() => setStarting(null), GIVE_UP_MS);
    return () => clearTimeout(timer);
  }, [starting]);

  const profile = profiles.data?.find((p) => p.id === starting);
  if (!profile) return null;
  return <Starting profile={profile} onClose={() => setStarting(null)} />;
}

function Starting({ profile, onClose }: { profile: Profile; onClose: () => void }) {
  const { t } = useTranslation();
  const games = useGames();
  const cover = useCover(profile.id, profile.installId);
  const game = games.data?.find((g) => g.id === profile.gameId);
  return (
    <div
      role="status"
      className="bb-fade fixed inset-0 z-50 flex items-center justify-center bg-bg/60 p-8 backdrop-blur-sm"
      onMouseDown={onClose}
    >
      <div className="bb-rise w-[min(32rem,80vw,90vh)] overflow-hidden rounded-xl border border-border bg-surface shadow-2xl">
        <Cover
          gameId={profile.gameId}
          game={game}
          src={cover.data}
          className="block aspect-[4/3] w-full text-6xl"
        />
        <div className="h-0.5 overflow-hidden bg-surface-raised">
          <div className="bb-sweep h-full w-1/3 bg-accent" />
        </div>
        <div className="flex items-center gap-3 px-4 py-3">
          <Bloog className="bb-hop size-6 shrink-0" />
          <span className="line-clamp-2 text-sm text-muted">
            {t("launch.starting", { name: profile.label })}
          </span>
        </div>
      </div>
    </div>
  );
}
