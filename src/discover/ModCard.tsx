import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, ExternalLink, RefreshCw, TriangleAlert } from "lucide-react";
import { Fragment, type RefObject, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Game, type IndexMod, type Install } from "../bindings";
import { bestFit, gameTitle, shortGameTitle } from "../catalog";
import { formatDownloadSize, looseDateText } from "../format";
import { JobRow } from "../jobs/JobList";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { DetailsLink } from "./DetailsLink";
import { entryDescription } from "./description";
import { GameTile } from "./GameTile";
import { Authors } from "../mods/Authors";
import { WhyExactVersion } from "../mods/OtherVersion";
import { ownedVersions } from "../mods/ownedVersions";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Dialog } from "../ui/Dialog";
import { useEnterAction } from "../ui/useEnterAction";
import { fitting, freeFit, isSourceMod, needsNoGame, requiredLabels } from "./fit";
import { useFreeGames, useIndexMods } from "./queries";
import { StatusBadge } from "./StatusBadge";
import { SeriesLine } from "../series/SeriesLine";
import { episodeTitle, seriesOf, useSeriesPlaces } from "../series/series";

/** A mod from the index with everything needed to install it. */
export function ModCard({
  mod,
  installs,
  games,
  archive,
  enterInstalls = false,
  heading: Heading = "h2",
  featuredBadge = true,
  seriesLine = true,
}: {
  mod: IndexMod;
  installs: Install[];
  games: Game[];
  /** The player's own copy of the download, installed instead of downloading it. */
  archive?: string;
  /** Enter installs or updates it, unless there's a warning to read. */
  enterInstalls?: boolean;
  /** Its name's heading: h3 under a group's. */
  heading?: "h2" | "h3";
  /** Off under the "Recommended" group's heading, which says it already. */
  featuredBadge?: boolean;
  /** Off on the series' own page, which says it already. */
  seriesLine?: boolean;
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
  const description = entryDescription(entry, i18n.language);
  const place = seriesOf(useSeriesPlaces(), { indexId: entry.id });
  const date = entry.updated ?? entry.released;
  const details = [
    entry.authors.length > 0 && <Authors authors={entry.authors} links={entry.authorLinks} />,
    entry.version && t("mods.version", { version: entry.version }),
    // The shown version's date when known, else the first release's; with the
    // version beside it, "released" fits both.
    date && looseDateText(t, i18n.language, "mods.releasedOn", date),
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
          <GameTile gameId={entry.game} game={game} standalone={isSourceMod(entry)} />
          <div className="grid min-w-0 content-start gap-1">
            {place && seriesLine && <SeriesLine place={place} link />}
            <Heading className="leading-tight font-semibold">
              {place ? episodeTitle(place.part, entry.name) : entry.name}
            </Heading>
            <p className="text-xs text-muted">
              {/* A source mod is named here once, not again as a badge. */}
              {isSourceMod(entry)
                ? `${t("mods.sourceMod")} · ${t("mods.standalone")}`
                : needsNoGame(entry)
                  ? `${gameTitle(games, entry.game)} · ${t("mods.standalone")}`
                  : gameTitle(games, entry.game)}
            </p>
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
          <StatusBadge status={entry.status} />
          {entry.worlds && <Badge>{t("mods.randomWorlds")}</Badge>}
          {mod.new && <Badge>{t("mods.new")}</Badge>}
          {entry.featured && featuredBadge && <Badge>{t("discover.featured")}</Badge>}
        </div>
        {description && <p className="text-sm">{description}</p>}
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
  const { t, i18n } = useTranslation();
  const { entry } = mod;
  const job = useJob("mod", entry.id);
  const start = useStartJob((installId: string | null) =>
    commands.installIndexMod(entry.id, installId, installId === null, archive ?? null),
  );
  const [confirming, setConfirming] = useState(false);
  const candidates = fitting(entry, installs, games);
  const game = games.find((g) => g.id === entry.game);
  // Without a fitting copy, a free one comes along when there is one.
  const freeGames = useFreeGames();
  const free =
    candidates.length === 0 && !needsNoGame(entry)
      ? freeFit(entry.requires, game, freeGames.data ?? [])
      : undefined;
  // Remembered, as it's no longer offered once it's in the library.
  const [withFree, setWithFree] = useState<string | null>(null);
  const freeJob = useJob("freeGame", withFree);
  const startFree = useStartJob((versionId: string) => commands.installFreeGame(versionId));
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
    : t("discover.install", { size: formatDownloadSize(i18n.language, size) });
  const busy = start.isPending || (job !== undefined && !isFinished(job));
  // Not without the game: that comes with a warning.
  const update = mod.modId && mod.updateAvailable;
  // Standalone mods go on no copy.
  const enterInstall = update || needsNoGame(entry) ? null : !mod.modId ? target?.id : undefined;
  useEnterAction(
    enter,
    !busy && enterInstall !== undefined ? () => start.mutate(enterInstall) : null,
  );

  const freeLabel = (versionId: string) =>
    `${game?.shortTitle ?? entry.game} ${game?.versions.find((v) => v.id === versionId)?.label ?? ""}`.trim();
  const freeRow = freeJob &&
    withFree &&
    (freeJob.phase !== "done" || (job && !isFinished(job))) && (
      <JobRow job={freeJob} label={freeLabel(withFree)} />
    );
  if (job && !isFinished(job)) {
    return (
      <>
        {freeRow}
        <JobRow job={job} label={entry.name} />
      </>
    );
  }
  const failed = (
    <>
      {freeJob?.phase === "failed" && freeRow}
      {job?.phase === "failed" && <JobRow job={job} label={entry.name} />}
    </>
  );

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
  if (needsNoGame(entry)) {
    return (
      <>
        {failed}
        <Button variant="card" onClick={() => start.mutate(null)} disabled={start.isPending}>
          <Download className="size-4" aria-hidden />
          {installLabel}
        </Button>
      </>
    );
  }
  if (!target && free) {
    const installWithFree = async () => {
      setWithFree(free.versionId);
      // The mod's job waits for the game and then goes on it.
      await startFree.mutateAsync(free.versionId);
      start.mutate(null);
    };
    return (
      <>
        {failed}
        <p className="text-xs text-muted">
          {t("discover.withShareware", { game: freeLabel(free.versionId) })}
        </p>
        <Button
          variant="card"
          onClick={installWithFree}
          disabled={start.isPending || startFree.isPending}
        >
          <Download className="size-4" aria-hidden />
          {t("discover.install", { size: formatDownloadSize(i18n.language, size + free.size) })}
        </Button>
      </>
    );
  }
  if (!target) {
    const versions = requiredLabels(entry, games);
    // A copy of another version: say which, and why it won't do.
    const yours =
      versions.length > 0
        ? ownedVersions(
            installs,
            entry.game,
            games.find((g) => g.id === entry.game),
          )
        : null;
    const game = versions.length
      ? `${gameTitle(games, entry.game)} (${versions.join(", ")})`
      : gameTitle(games, entry.game);
    return (
      <>
        {failed}
        <p role="status" className="flex items-start gap-2 text-sm text-muted">
          <TriangleAlert className="mt-0.5 size-4 shrink-0 text-warning" aria-hidden />
          <span>
            {t("discover.needs", {
              game: gameTitle(games, entry.game),
              versions: versions.join(", "),
              context: versions.length > 0 ? "version" : undefined,
            })}
            {yours && (
              <>
                {" "}
                {t("mods.yourCopy", { versions: yours })}{" "}
                <WhyExactVersion requires={entry.requires} />
              </>
            )}
          </span>
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
