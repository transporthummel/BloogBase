import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type FreeGame, type Game } from "../bindings";
import { gameTitle } from "../catalog";
import { errorMessage } from "../errors";
import { formatMegabytes } from "../format";
import { unwrap } from "../ipc";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { GameTile } from "./GameTile";
import { DetailsLink } from "./DetailsLink";
import { FindOwnedDialog } from "../library/FindOwnedDialog";
import { profilesKey } from "../library/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { STORE_LINKS } from "../storeLinks";
import { useFreeGames } from "./queries";

/** Full shareware episodes, or the demos and betas. */
export type FreeCategory = "games" | "demos";

/** The free versions of one category, in catalog order (PLAN.md §2.3). */
export function FreeGames({ category, games }: { category: FreeCategory; games: Game[] }) {
  const { t } = useTranslation();
  const free = useFreeGames();
  const [finding, setFinding] = useState(false);
  const [imported, setImported] = useState<number | null>(null);
  const shown = (free.data ?? [])
    .map((f) => {
      const game = games.find((g) => g.id === f.gameId);
      const version = game?.versions.find((v) => v.id === f.versionId);
      return { free: f, game, kind: version?.kind ?? "release" };
    })
    .filter(({ kind }) => (category === "games") === (kind === "release"))
    .sort((a, b) => games.indexOf(a.game!) - games.indexOf(b.game!));

  return (
    <div className="grid gap-4">
      <p className="max-w-2xl text-sm text-muted">{t(`discover.free.intro.${category}`)}</p>
      {free.error && <Notice tone="error">{errorMessage(t, free.error)}</Notice>}
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(16rem,1fr))] gap-4">
        {shown.map(({ free: f, game }) => (
          <li key={f.versionId}>
            <FreeGameCard free={f} game={game} games={games} />
          </li>
        ))}
      </ul>
      {/* The shareware is only part of it; where the rest comes from. */}
      {category === "games" && (
        <div className="grid max-w-2xl gap-1 text-sm">
          <p className="text-muted">{t("discover.free.buy")}</p>
          <p className="flex flex-wrap items-center gap-4">
            <Button variant="secondary" onClick={() => setFinding(true)}>
              <Search className="size-4" aria-hidden />
              {t("library.findOwned")}
            </Button>
            {(["gog", "steam"] as const).map((store) => (
              <button
                key={store}
                type="button"
                className="inline-flex cursor-pointer items-center gap-1 text-ink underline underline-offset-2"
                onClick={() => openUrl(STORE_LINKS[store])}
              >
                {t(`setup.owned.store.${store}`)}
                <ExternalLink className="size-3.5" aria-hidden />
              </button>
            ))}
          </p>
          {imported !== null && <Notice>{t("library.imported", { count: imported })}</Notice>}
        </div>
      )}
      {finding && (
        <FindOwnedDialog
          onClose={() => setFinding(false)}
          onImported={(count) => {
            setFinding(false);
            setImported(count);
          }}
        />
      )}
      <p className="max-w-2xl text-xs text-muted">{t(`discover.free.terms.${category}`)}</p>
    </div>
  );
}

function FreeGameCard({
  free,
  game,
  games,
}: {
  free: FreeGame;
  game: Game | undefined;
  games: Game[];
}) {
  const { t } = useTranslation();
  const version = game?.versions.find((v) => v.id === free.versionId);
  const kind = version?.kind ?? "release";
  const label = version?.label ?? free.versionId;

  return (
    <Card className="flex h-full flex-col">
      <div className="flex flex-1 flex-col gap-3 p-4">
        <div className="flex gap-3">
          <GameTile gameId={free.gameId} game={game} />
          <h2 className="min-w-0 self-center leading-tight font-semibold">
            {gameTitle(games, free.gameId)}
          </h2>
        </div>
        <div className="flex flex-wrap gap-1">
          {free.installed && <Badge tone="status">{t("discover.installed")}</Badge>}
          {/* The version as a badge, as on library cards. */}
          <Badge>{label}</Badge>
          {version?.shareware && <Badge>{t("profile.shareware")}</Badge>}
          {/* Not twice, as in "November beta" and "Beta". */}
          {kind !== "release" &&
            !label.toLowerCase().includes(t(`discover.free.kind.${kind}`).toLowerCase()) && (
              <Badge>{t(`discover.free.kind.${kind}`)}</Badge>
            )}
        </div>
        <p className="text-sm">
          {t(`discover.free.about.${free.versionId}`, { defaultValue: "" })}
        </p>
        <div className="mt-auto grid grid-cols-1 gap-2 pt-1">
          <FreeGameAction
            free={free}
            label={`${gameTitle(games, free.gameId)} (${version?.label})`}
          />
        </div>
      </div>
    </Card>
  );
}

function FreeGameAction({ free, label }: { free: FreeGame; label: string }) {
  const { t } = useTranslation();
  const job = useJob("freeGame", free.versionId);
  const start = useStartJob((versionId: string) => commands.installFreeGame(versionId));
  const profiles = useQuery({
    queryKey: profilesKey,
    queryFn: () => unwrap(commands.listProfiles()),
    enabled: free.installed,
  });

  if (job && !isFinished(job)) return <JobRow job={job} label={label} />;
  if (free.installed) {
    // The game's own play profile, not one of a mod played on it.
    const profile = profiles.data?.find((p) => p.versionId === free.versionId && !p.modId);
    return profile ? <DetailsLink profileId={profile.id} /> : null;
  }
  return (
    <>
      {job?.phase === "failed" && <JobRow job={job} label={label} />}
      <Button
        variant="card"
        onClick={() => start.mutate(free.versionId)}
        disabled={start.isPending}
      >
        <Download className="size-4" aria-hidden />
        {t("discover.free.download", { size: formatMegabytes(free.size) })}
      </Button>
    </>
  );
}
