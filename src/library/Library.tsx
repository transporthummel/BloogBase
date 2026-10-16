import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import { FileArchive, FolderOpen, Play } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type Game, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { formatDate, formatPlayTime } from "../format";
import { unwrap } from "../ipc";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";

const profilesKey = ["profiles"] as const;

export function Library() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
  });
  const runtime = useRuntimeStatus();

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
    onSuccess: () => queryClient.invalidateQueries({ queryKey: profilesKey }),
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

  const gameById = new Map(games.data?.map((g) => [g.id, g]));
  const error = importGames.error ?? launch.error ?? profiles.error;
  const empty = profiles.data?.length === 0;
  const needsRuntime = !empty && runtime.data && !runtime.data.active;

  return (
    <>
      <PageHeader title={t("library.title")} actions={!empty && importButtons} />

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
            />
          </li>
        ))}
      </ul>
    </>
  );
}

const engineColor = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
} as const;

function ProfileCard({
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
  const version = game?.versions.find((v) => v.id === profile.versionId);
  // Placeholder cover: episode number on the engine's color.
  const episode = profile.gameId.replace(/^keen/, "").replace(/^kdreams$/, "D");
  return (
    <Card className="flex h-full flex-col">
      <div
        className={`flex aspect-[3/4] items-center justify-center font-pixel text-5xl text-white/90 ${
          game ? engineColor[game.engine] : "bg-surface-raised"
        }`}
        aria-hidden
      >
        {episode}
      </div>
      <div className="flex flex-1 flex-col gap-2 p-3">
        <h2 className="text-sm leading-tight font-semibold">{profile.label}</h2>
        <div>
          <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
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
          <dd>{t("library.saveCount", { count: profile.saveCount })}</dd>
        </dl>
        <Button
          className="mt-auto"
          onClick={onPlay}
          disabled={!canPlay || profile.running}
          aria-label={t("library.playGame", { game: profile.label })}
        >
          <Play className="size-4" aria-hidden />
          {profile.running ? t("library.running") : t("library.play")}
        </Button>
      </div>
    </Card>
  );
}
