import { Link } from "@tanstack/react-router";
import { memo, useCallback } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { type Game, type Profile, type WorldStart } from "../bindings";
import { StatusBadge } from "../discover/StatusBadge";
import { useIndexMods } from "../discover/queries";
import { formatPlayTime } from "../format";
import { RunsOn } from "../mods/RunsOn";
import { usePlayChoices } from "../profile/playChoices";
import { variantName } from "../profile/variants";
import { SeriesLine } from "../series/SeriesLine";
import { episodeTitle, type SeriesPlace } from "../series/series";
import { Badge } from "../ui/Badge";
import { Card } from "../ui/Card";
import { PlayButton } from "../ui/PlayButton";
import { RelativeTime } from "../ui/RelativeTime";
import { SplitButton } from "../ui/SplitButton";
import { Cover } from "./Cover";
import { useCover } from "./covers";

// Memoized so a refetch only redraws the cards whose profile changed.
export const ProfileCard = memo(function ProfileCard({
  profile,
  game,
  heading: Heading,
  sharedLabel,
  canPlay,
  updating,
  series,
  seriesLine = true,
  large = false,
  onPlay,
}: {
  profile: Profile;
  game?: Game;
  /** h3 under a group heading. */
  heading: "h2" | "h3";
  /** Another card has the same name. */
  sharedLabel: boolean;
  canPlay: boolean;
  /** An update is swapping the mod's files. */
  updating: boolean;
  /** Its place in a series, the same object while the series don't change. */
  series?: SeriesPlace;
  /** Off on the series' own page, which says it already. */
  seriesLine?: boolean;
  /** Bigger, with more of the cover: the series' page has only a few parts. */
  large?: boolean;
  /** With the world to play, for a mod that keeps worlds, or a variant. */
  onPlay: (profile: Profile, world: WorldStart | null, variant?: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const play = useCallback(
    (world: WorldStart | null, variant?: string) => onPlay(profile, world, variant),
    [onPlay, profile],
  );
  const version = game?.versions.find((v) => v.id === profile.versionId);
  const versionLabel = version?.label ?? t("library.unknownVersion");
  // The badge tells copies apart on screen; screen readers hear the version
  // with the name.
  // In a series, the episode's own title below the series' line.
  const title = series ? episodeTitle(series.part, profile.label) : profile.label;
  const name = sharedLabel ? `${title} (${versionLabel})` : title;
  const cover = useCover(profile.id, profile.installId, profile.activeVariant?.id ?? null);
  // Alpha, beta or abandoned, as the index says now, else the KeenWiki page.
  const status =
    useIndexMods().data?.find((m) => profile.modId !== null && m.modId === profile.modId)?.entry
      .status ?? profile.modStatus;
  return (
    // Only the border colour changes, so nothing inside the card moves.
    <Card className="flex h-full flex-col transition-colors has-[a:hover]:border-accent/35">
      {/* A second way to the same page for the mouse; the title link below
          is the one for keyboards and screen readers. */}
      <Link
        to="/profiles/$profileId"
        params={{ profileId: profile.id }}
        tabIndex={-1}
        aria-hidden
        className="block"
      >
        <Cover
          gameId={profile.gameId}
          game={game}
          src={cover.data?.src}
          onError={cover.onError}
          standalone={profile.sourceMod}
          className={large ? LARGE_COVER : undefined}
        />
      </Link>
      <div className={`flex flex-1 flex-col gap-2 ${large ? "p-4" : "p-3"}`}>
        <div className="grid gap-0.5">
          {series && seriesLine && <SeriesLine place={series} link />}
          <Heading className={`${large ? "text-base" : "text-sm"} leading-tight font-semibold`}>
            <Link
              to="/profiles/$profileId"
              params={{ profileId: profile.id }}
              // A block, so the focus ring is one box around a wrapped title.
              className="block hover:underline"
              title={t("library.details")}
              // Also keeps the no-break spaces of the wrapped title out of its name.
              aria-label={name}
            >
              {keepSeriesTogether(title)}
            </Link>
          </Heading>
          {(() => {
            // The variant played, then what it runs on, in one line; the
            // variant's name wraps as a whole.
            const runsOn =
              profile.missingGame !== null ? (
                t("mods.waitsFor", { game: profile.missingGame })
              ) : profile.modId !== null && game ? (
                profile.sourceMod ? (
                  t("mods.standalone")
                ) : (
                  <RunsOn profile={profile} game={game.shortTitle} version={versionLabel} />
                )
              ) : null;
            if (!runsOn && !profile.activeVariant) return null;
            return (
              <p className="text-xs text-muted">
                {profile.activeVariant && (
                  // Leads to it in the Variants tab, as the game links to its page.
                  <Link
                    to="/profiles/$profileId"
                    params={{ profileId: profile.id }}
                    search={{ show: "variant" }}
                    title={t("variants.showAll")}
                    className="inline-block underline-offset-2 hover:text-accent hover:underline"
                  >
                    <span className="sr-only">{t("variants.variant")} </span>
                    {variantName(profile.activeVariant, i18n.language)}
                  </Link>
                )}
                {runsOn && profile.activeVariant && " · "}
                {runsOn}
              </p>
            );
          })()}
        </div>
        <div className="flex flex-wrap gap-1">
          {profile.modId !== null && !profile.sourceMod && <Badge>{t("mods.badge")}</Badge>}
          {profile.sourceMod && <Badge>{t("mods.sourceMod")}</Badge>}
          {profile.worlds && <Badge>{t("mods.randomWorlds")}</Badge>}
          {profile.variantCount > 0 && (
            <Badge>{t("variants.badge", { count: profile.variantCount })}</Badge>
          )}
          <StatusBadge status={status} />
          {profile.modId === null && <Badge>{versionLabel}</Badge>}
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
                <RelativeTime at={profile.lastPlayedAt}>
                  {(date) => t("library.lastPlayedOn", { date })}
                </RelativeTime>
              </dd>
            </>
          )}
          {/* No line for no saves: the card says enough without it. */}
          {profile.saveCount > 0 && (
            <>
              <dt className="sr-only">{t("library.saveCountLabel")}</dt>
              <dd>
                {/* Leads where the title does: for the mouse only, so keyboards
                    stop once per card before Play. */}
                <Link
                  to="/profiles/$profileId"
                  params={{ profileId: profile.id }}
                  search={{ show: "saves" }}
                  tabIndex={-1}
                  className="underline-offset-2 hover:underline"
                >
                  {t("library.saveCount", { count: profile.saveCount })}
                </Link>
              </dd>
            </>
          )}
        </dl>
        <div className="mt-auto flex gap-2 pt-1.5">
          {profile.variantCount > 0 || profile.worlds ? (
            <CardChoices
              profile={profile}
              name={name}
              canPlay={canPlay}
              updating={updating}
              play={play}
            />
          ) : (
            <PlayButton
              variant="card"
              className="flex-1"
              onClick={() => play(null)}
              disabled={!canPlay || profile.running || updating || profile.missingGame !== null}
              aria-label={t("library.playGame", { game: name })}
            >
              {profile.missingGame !== null
                ? t("mods.gameMissing")
                : updating
                  ? t("library.updating")
                  : profile.running
                    ? t("library.running")
                    : t("library.play")}
            </PlayButton>
          )}
        </div>
      </div>
    </Card>
  );
});

/** The cover of a large card, with a bigger placeholder number. */
export const LARGE_COVER = "aspect-[4/3] text-7xl";

/**
 * Play with the other variants and worlds beside it, as on the page. Only
 * for cards that have any: the queries and labels behind it cost every one
 * of hundreds of cards.
 */
function CardChoices({
  profile,
  name,
  canPlay,
  updating,
  play,
}: {
  profile: Profile;
  name: string;
  canPlay: boolean;
  updating: boolean;
  play: (world: WorldStart | null, variant?: string) => void;
}) {
  const { t } = useTranslation();
  const choices = usePlayChoices(profile, play, { short: true });
  return (
    <>
      {choices.split && !profile.running && !updating && profile.missingGame === null ? (
        <SplitButton
          variant="card"
          // Narrow cards: both halves fit inside, the label on one line.
          wrapperClassName="min-w-0 flex-1"
          className="min-w-0 flex-1 px-2! whitespace-nowrap"
          onClick={choices.split.play}
          disabled={!canPlay}
          aria-label={t("library.playGame", { game: name })}
          menuLabel={t(`library.${choices.split.kind}More`, { game: name })}
          items={choices.split.items}
        >
          {choices.split.label}
        </SplitButton>
      ) : (
        <PlayButton
          variant="card"
          className="flex-1"
          onClick={() => (profile.worlds ? choices.newWorld() : play(null))}
          disabled={!canPlay || profile.running || updating || profile.missingGame !== null}
          aria-label={t("library.playGame", { game: name })}
        >
          {profile.missingGame !== null
            ? t("mods.gameMissing")
            : updating
              ? t("library.updating")
              : profile.running
                ? t("library.running")
                : profile.worlds
                  ? choices.newLabel
                  : t("library.play")}
        </PlayButton>
      )}
      {/* Out of the card, whose containment would clip a fixed dialog. */}
      {choices.dialog && createPortal(choices.dialog, document.body)}
    </>
  );
}

/**
 * "Commander Keen 5:" stays on one line, so a narrow card wraps after the
 * episode rather than between "Commander" and "Keen".
 */
function keepSeriesTogether(label: string) {
  return label.replace(/^Commander Keen (\d)/, "Commander\u00a0Keen\u00a0$1");
}
