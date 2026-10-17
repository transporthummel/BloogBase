import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useParams } from "@tanstack/react-router";
import { ArrowLeft, Play } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events } from "../bindings";
import { useGames } from "../catalog";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { profilesKey } from "../library/queries";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { ConfigPanel } from "./ConfigPanel";
import { SavesPanel } from "./SavesPanel";
import { backupsKey, savesKey } from "./saves";

type Tab = "saves" | "config";

export function ProfileRoute() {
  const { profileId } = useParams({ from: "/profiles/$profileId" });
  return <ProfileDetail profileId={profileId} />;
}

/** One game or mod: play it, manage its saves and DOSBox settings (§8.3). */
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
    // The game may have saved, and made an automatic backup before.
    const unlisten = events.playSessionEnded.listen(() => {
      queryClient.invalidateQueries({ queryKey: profilesKey });
      queryClient.invalidateQueries({ queryKey: savesKey(profileId) });
      queryClient.invalidateQueries({ queryKey: backupsKey(profileId) });
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
  const version = game?.versions.find((v) => v.id === profile?.versionId);

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
    { id: "config", label: t("profile.tabConfig") },
  ];

  return (
    <>
      <BackLink />
      <PageHeader
        title={profile?.label ?? ""}
        actions={
          profile && (
            <Button
              onClick={() => launch.mutate()}
              disabled={!runtime.data?.active || profile.running || launch.isPending}
            >
              <Play className="size-4" aria-hidden />
              {profile.running ? t("library.running") : t("library.play")}
            </Button>
          )
        }
      />
      {profile && (
        <div className="mb-4 flex flex-wrap items-center gap-2 text-sm text-muted">
          {profile.modId !== null && game && <span>{game.title}</span>}
          <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
        </div>
      )}
      {launch.error && <Notice tone="error">{errorMessage(t, launch.error)}</Notice>}

      <div role="tablist" className="mb-4 flex gap-1 border-b border-border">
        {tabs.map((x) => (
          <button
            key={x.id}
            role="tab"
            type="button"
            id={`tab-${x.id}`}
            aria-selected={tab === x.id}
            aria-controls={`panel-${x.id}`}
            onClick={() => setTab(x.id)}
            className={`-mb-px border-b-2 px-3 py-2 text-sm font-semibold ${
              tab === x.id
                ? "border-accent text-ink"
                : "border-transparent text-muted hover:text-ink"
            }`}
          >
            {x.label}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`panel-${tab}`} aria-labelledby={`tab-${tab}`} className="max-w-3xl">
        {profile && tab === "saves" && <SavesPanel profile={profile} game={game} />}
        {profile && tab === "config" && <ConfigPanel profileId={profile.id} />}
      </div>
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
