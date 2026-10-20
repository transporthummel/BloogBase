import { Link } from "@tanstack/react-router";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, RefreshCw } from "lucide-react";
import { Fragment, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Game, type IndexMod, type Install } from "../bindings";
import { bestFit, gameTitle } from "../catalog";
import { formatMegabytes } from "../format";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { Cover } from "../library/Cover";
import { Authors } from "../mods/Authors";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { fitting, isNew, requiredLabels } from "./fit";

/** A mod from the index with everything needed to install it. */
export function ModCard({
  mod,
  installs,
  games,
}: {
  mod: IndexMod;
  installs: Install[];
  games: Game[];
}) {
  const { t } = useTranslation();
  const { entry } = mod;
  const game = games.find((g) => g.id === entry.game);
  const latest = entry.downloads[0];
  const details = [
    entry.authors.length > 0 && <Authors authors={entry.authors} links={entry.authorLinks} />,
    entry.version && t("mods.version", { version: entry.version }),
    entry.updated ?? entry.released,
  ].filter(Boolean);
  const links = [
    ["KeenWiki", entry.links.keenwiki],
    ["PCKF", entry.links.pckf],
    [t("discover.homepage"), entry.links.homepage],
  ].filter((l): l is [string, string] => !!l[1]);

  return (
    <Card className="flex h-full flex-col">
      <Cover gameId={entry.game} game={game} className="h-20 text-3xl" />
      <div className="flex flex-1 flex-col gap-3 p-4">
        <div className="grid gap-1">
          <h2 className="leading-tight font-semibold">{entry.name}</h2>
          <p className="text-xs text-muted">{gameTitle(games, entry.game)}</p>
        </div>
        <div className="flex flex-wrap gap-1">
          <Badge>{t(`discover.kind.${entry.type}`)}</Badge>
          {isNew(entry) && <Badge>{t("mods.new")}</Badge>}
          {entry.featured && <Badge>{t("discover.featured")}</Badge>}
          {mod.modId && <Badge>{t("discover.installed")}</Badge>}
          {mod.updateAvailable && <Badge>{t("discover.updateBadge")}</Badge>}
        </div>
        {entry.description && <p className="text-sm">{entry.description}</p>}
        {details.length > 0 && (
          <p className="text-xs text-muted">
            {details.map((d, i) => (
              <Fragment key={i}>
                {i > 0 && " · "}
                {d}
              </Fragment>
            ))}
          </p>
        )}
        {links.length > 0 && (
          <p className="flex flex-wrap gap-3 text-xs">
            {links.map(([label, url]) => (
              <button
                key={label}
                type="button"
                className="inline-flex items-center gap-1 text-ink underline underline-offset-2"
                onClick={() => openUrl(url)}
              >
                {label}
                <ExternalLink className="size-3" aria-hidden />
              </button>
            ))}
          </p>
        )}
        <div className="mt-auto grid grid-cols-1 gap-2 pt-1">
          <InstallAction mod={mod} installs={installs} games={games} size={latest?.size ?? 0} />
        </div>
      </div>
    </Card>
  );
}

function InstallAction({
  mod,
  installs,
  games,
  size,
}: {
  mod: IndexMod;
  installs: Install[];
  games: Game[];
  size: number;
}) {
  const { t } = useTranslation();
  const { entry } = mod;
  const job = useJob("mod", entry.id);
  const start = useStartJob((installId: string | null) =>
    commands.installIndexMod(entry.id, installId, installId === null),
  );
  const [confirming, setConfirming] = useState(false);
  const candidates = fitting(entry, installs, games);
  const [installId, setInstallId] = useState<string | null>(null);
  const target =
    candidates.find((i) => i.id === installId) ??
    bestFit(
      games.find((g) => g.id === entry.game),
      candidates,
    );
  const versionLabel = (i: Install) =>
    games.find((g) => g.id === i.gameId)?.versions.find((v) => v.id === i.versionId)?.label ??
    t("library.unknownVersion");

  if (job && !isFinished(job)) return <JobRow job={job} label={entry.name} />;
  const failed = job?.phase === "failed" && <JobRow job={job} label={entry.name} />;

  if (mod.modId && !mod.updateAvailable) {
    return (
      // A mod's play profile has the mod's id.
      <Link
        to="/profiles/$profileId"
        params={{ profileId: mod.modId }}
        className="text-sm text-ink underline underline-offset-2"
      >
        {t("discover.openDetails")}
      </Link>
    );
  }
  if (mod.modId) {
    return (
      <>
        {failed}
        <Button variant="secondary" onClick={() => start.mutate(null)} disabled={start.isPending}>
          <RefreshCw className="size-4" aria-hidden />
          {entry.version
            ? t("discover.updateTo", { version: entry.version })
            : t("discover.update")}
        </Button>
      </>
    );
  }
  if (!target) {
    const versions = requiredLabels(entry, games);
    const game = versions.length
      ? `${gameTitle(games, entry.game)} (${versions.join(", ")})`
      : gameTitle(games, entry.game);
    return (
      <>
        {failed}
        <Notice>
          {t("discover.needs", {
            game: gameTitle(games, entry.game),
            versions: versions.join(", "),
            context: versions.length > 0 ? "version" : undefined,
          })}
        </Notice>
        <Button variant="secondary" onClick={() => setConfirming(true)} disabled={start.isPending}>
          <Download className="size-4" aria-hidden />
          {t("discover.installAnyway")}
        </Button>
        {confirming && (
          <Dialog
            title={t("discover.withoutGameTitle", { name: entry.name })}
            onClose={() => setConfirming(false)}
          >
            <p className="text-sm text-muted">{t("discover.withoutGameHint", { game })}</p>
            <div className="flex justify-end gap-3">
              <Button variant="ghost" onClick={() => setConfirming(false)}>
                {t("common.cancel")}
              </Button>
              <Button
                onClick={() => {
                  setConfirming(false);
                  start.mutate(null);
                }}
              >
                {t("discover.install", { size: formatMegabytes(size) })}
              </Button>
            </div>
          </Dialog>
        )}
      </>
    );
  }
  return (
    <>
      {failed}
      {candidates.length > 1 && (
        <label className="grid grid-cols-1 gap-1 text-xs">
          <span className="text-muted">{t("mods.playOn")}</span>
          <select
            className="min-w-0 truncate rounded-lg border border-border bg-surface-raised px-2 py-1.5 text-sm"
            value={target.id}
            onChange={(e) => setInstallId(e.target.value)}
          >
            {candidates.map((i) => (
              <option key={i.id} value={i.id}>
                {i.label} ({versionLabel(i)})
              </option>
            ))}
          </select>
        </label>
      )}
      <Button onClick={() => start.mutate(target.id)} disabled={start.isPending}>
        <Download className="size-4" aria-hidden />
        {t("discover.install", { size: formatMegabytes(size) })}
      </Button>
    </>
  );
}
