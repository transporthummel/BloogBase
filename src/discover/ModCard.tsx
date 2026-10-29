import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, RefreshCw, TriangleAlert } from "lucide-react";
import { Fragment, type RefObject, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Game, type IndexMod, type Install } from "../bindings";
import { bestFit, gameTitle, shortGameTitle } from "../catalog";
import { formatLooseDate, formatMegabytes } from "../format";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { DetailsLink } from "./DetailsLink";
import { GameTile } from "./GameTile";
import { Authors } from "../mods/Authors";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Dialog } from "../ui/Dialog";
import { useEnterAction } from "../ui/useEnterAction";
import { fitting, requiredLabels } from "./fit";
import { useIndexMods } from "./queries";

/** A mod from the index with everything needed to install it. */
export function ModCard({
  mod,
  installs,
  games,
  archive,
  enterInstalls = false,
}: {
  mod: IndexMod;
  installs: Install[];
  games: Game[];
  /** The player's own copy of the download, installed instead of downloading it. */
  archive?: string;
  /** Enter installs or updates it, unless there's a warning to read. */
  enterInstalls?: boolean;
}) {
  const { t, i18n } = useTranslation();
  const actions = useRef<HTMLDivElement>(null);
  const { entry } = mod;
  const game = games.find((g) => g.id === entry.game);
  const latest = entry.downloads[0];
  // The mod it's played over, installed along with it when missing.
  const index = useIndexMods();
  const base = entry.extends ? index.data?.find((m) => m.entry.id === entry.extends) : undefined;
  const baseMissing = base !== undefined && base.modId === null;
  const date = entry.updated ?? entry.released;
  const details = [
    entry.authors.length > 0 && <Authors authors={entry.authors} links={entry.authorLinks} />,
    entry.version && t("mods.version", { version: entry.version }),
    date && formatLooseDate(i18n.language, date),
  ].filter(Boolean);
  const links = [
    ["KeenWiki", entry.links.keenwiki],
    ["PCKF", entry.links.pckf],
    ["Keen: Modding", entry.links.keenmodding],
    [t("discover.homepage"), entry.links.homepage],
  ].filter((l): l is [string, string] => !!l[1]);

  return (
    <Card className="flex h-full flex-col">
      <div className="flex flex-1 flex-col gap-3 p-4">
        <div className="flex gap-3">
          <GameTile gameId={entry.game} game={game} />
          <div className="grid min-w-0 content-start gap-1">
            <h2 className="leading-tight font-semibold">{entry.name}</h2>
            <p className="text-xs text-muted">{gameTitle(games, entry.game)}</p>
            {base && (
              <p className="text-xs text-muted">
                {t(baseMissing ? "discover.extendsInstall" : "discover.extends", {
                  name: base.entry.name,
                })}
              </p>
            )}
          </div>
        </div>
        <div className="flex flex-wrap gap-1">
          {/* The player's own state first. */}
          {mod.modId && <Badge tone="status">{t("discover.installed")}</Badge>}
          {mod.updateAvailable && <Badge tone="status">{t("discover.updateBadge")}</Badge>}
          <Badge>{t(`discover.kind.${entry.type}`)}</Badge>
          {mod.new && <Badge>{t("mods.new")}</Badge>}
          {entry.featured && <Badge>{t("discover.featured")}</Badge>}
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
                className="inline-flex cursor-pointer items-center gap-1 text-ink underline underline-offset-2"
                onClick={() => openUrl(url)}
              >
                {label}
                <ExternalLink className="size-3" aria-hidden />
              </button>
            ))}
          </p>
        )}
        <div ref={actions} className="mt-auto grid grid-cols-1 gap-2 pt-1">
          <InstallAction
            enter={enterInstalls ? actions : null}
            mod={mod}
            installs={installs}
            games={games}
            size={(latest?.size ?? 0) + (baseMissing ? (base.entry.downloads[0]?.size ?? 0) : 0)}
            archive={archive}
          />
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
  archive,
  enter,
}: {
  mod: IndexMod;
  installs: Install[];
  games: Game[];
  size: number;
  archive?: string;
  /** Where Enter installs, if it does. */
  enter: RefObject<HTMLDivElement | null> | null;
}) {
  const { t } = useTranslation();
  const { entry } = mod;
  const job = useJob("mod", entry.id);
  const start = useStartJob((installId: string | null) =>
    commands.installIndexMod(entry.id, installId, installId === null, archive ?? null),
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
  // A copy the player has needs no download.
  const installLabel = archive
    ? t("mods.install")
    : t("discover.install", { size: formatMegabytes(size) });
  const busy = start.isPending || (job !== undefined && !isFinished(job));
  // Not without the game: that comes with a warning.
  const update = mod.modId && mod.updateAvailable;
  const enterInstall = update ? null : !mod.modId ? target?.id : undefined;
  useEnterAction(
    enter,
    !busy && enterInstall !== undefined ? () => start.mutate(enterInstall) : null,
  );

  if (job && !isFinished(job)) return <JobRow job={job} label={entry.name} />;
  const failed = job?.phase === "failed" && <JobRow job={job} label={entry.name} />;

  if (mod.modId && !mod.updateAvailable) {
    return (
      // A mod's play profile has the mod's id.
      <DetailsLink profileId={mod.modId} />
    );
  }
  if (mod.modId) {
    return (
      <>
        {failed}
        <Button variant="card" onClick={() => start.mutate(null)} disabled={start.isPending}>
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
        <p role="status" className="flex items-start gap-2 text-sm text-muted">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
          {t("discover.needs", {
            game: gameTitle(games, entry.game),
            versions: versions.join(", "),
            context: versions.length > 0 ? "version" : undefined,
          })}
        </p>
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
                {installLabel}
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
                {shortGameTitle(games, i.gameId)} ({versionLabel(i)})
              </option>
            ))}
          </select>
        </label>
      )}
      <Button variant="card" onClick={() => start.mutate(target.id)} disabled={start.isPending}>
        <Download className="size-4" aria-hidden />
        {installLabel}
      </Button>
    </>
  );
}
