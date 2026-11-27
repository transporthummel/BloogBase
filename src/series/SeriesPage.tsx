import { Link, useParams, useRouter, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Check, Download, LibraryBig } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { type Game, type IndexMod, type Install } from "../bindings";
import { DEVELOPER, DEVELOPER_URL, gameTitle, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { isSourceMod, needsNoGame } from "../discover/fit";
import { InstallAction } from "../discover/ModCard";
import { useFreeGames, useIndexMods, useSeries } from "../discover/queries";
import { StatusBadge } from "../discover/StatusBadge";
import { useUpdatingMods } from "../jobs/queries";
import { Cover } from "../library/Cover";
import { LARGE_COVER, ProfileCard } from "../library/ProfileCard";
import { useInstalls, useProfiles } from "../library/queries";
import { useCardPlay } from "../library/useCardPlay";
import { Authors } from "../mods/Authors";
import { useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { buttonClass } from "../ui/buttonClass";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Loading } from "../ui/Loading";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { partStates, type PartState, seriesDescription, seriesName } from "./series";

export function SeriesRoute() {
  const { seriesId } = useParams({ from: "/series/$seriesId" });
  return <SeriesPage key={seriesId} seriesId={seriesId} />;
}

/**
 * A whole series: its parts in order, as cards like the library's. A part in
 * the library opens its page and plays from here; one not, installs.
 */
export function SeriesPage({ seriesId }: { seriesId: string }) {
  const { t, i18n } = useTranslation();
  const all = useSeries();
  const indexMods = useIndexMods();
  const profiles = useProfiles();
  const installs = useInstalls();
  const games = useGames().data;
  const runtime = useRuntimeStatus();
  const updating = useUpdatingMods();
  const { play, launch, dialogs } = useCardPlay(games);
  const hash = useRouterState({ select: (s) => s.location.hash });
  const series = all.data?.find((s) => s.id === seriesId);
  const ready = !!series && indexMods.isSuccess && profiles.isSuccess;

  // A part not in the library is shown here: scroll to it once it's drawn.
  useEffect(() => {
    const id = hash.replace(/^#/, "");
    if (ready && id) document.getElementById(id)?.scrollIntoView({ block: "start" });
  }, [ready, hash]);

  if (all.isPending || indexMods.isPending || profiles.isPending) return <Loading />;
  if (!series) {
    return (
      <EmptyState title={t("series.unknownTitle")}>
        <p className="max-w-md text-sm text-muted">{t("series.unknown")}</p>
        <Link to="/discover" className={buttonClass("secondary")}>
          {t("series.toDiscover")}
        </Link>
      </EmptyState>
    );
  }

  const states = partStates(series, indexMods.data ?? [], profiles.data ?? [], games);
  const episodes = states.slice(0, series.episodes.length);
  const bonus = states.slice(series.episodes.length);
  const authors = [
    ...new Set(
      states.flatMap((s) =>
        s.indexMod ? s.indexMod.entry.authors : s.part.game ? [DEVELOPER] : [],
      ),
    ),
  ];
  const authorLinks = Object.assign(
    { [DEVELOPER]: DEVELOPER_URL },
    ...states.map((s) => s.indexMod?.entry.authorLinks ?? {}),
  );
  const description = seriesDescription(series, i18n.language);
  const played = states.filter((s) => s.played).length;
  const canPlay = !!runtime.data?.active && !launch.isPending;

  const card = (s: PartState, index: number) => {
    const profile = profiles.data?.find((p) => p.id === s.profileId);
    const game = games?.find((g) => g.id === (profile?.gameId ?? s.part.game));
    if (profile) {
      return (
        <ProfileCard
          profile={profile}
          game={game}
          heading="h3"
          sharedLabel={false}
          canPlay={canPlay}
          updating={profile.modId !== null && updating.has(profile.modId)}
          series={{ series, part: s.part, number: s.number, index }}
          seriesLine={false}
          large
          onPlay={play}
        />
      );
    }
    if (s.indexMod) {
      return (
        <ModPartCard
          state={s}
          mod={s.indexMod}
          indexMods={indexMods.data ?? []}
          installs={installs.data ?? []}
          games={games ?? []}
        />
      );
    }
    return <GamePartCard state={s} game={game} />;
  };

  // Bigger than the library's, as a series has only a few parts: as many
  // side by side as it has episodes, at most three, so a wider window makes
  // them bigger rather than more. The bonus parts take the same width.
  const columns = Math.min(3, episodes.length);
  const list = (parts: PartState[], numbered: boolean, from: number) => (
    <ol
      className="grid gap-4"
      style={{
        gridTemplateColumns: `repeat(auto-fill, minmax(max(20rem, calc((100% - ${columns - 1}rem) / ${columns})), 1fr))`,
      }}
    >
      {parts.map((s, i) => (
        <li key={s.key} id={`part-${s.key}`} className="flex scroll-mt-6 gap-2">
          {numbered && (
            <span
              aria-hidden
              className="w-7 shrink-0 pt-2 text-right font-pixel text-base text-muted"
            >
              {s.number}
            </span>
          )}
          <div className="min-w-0 flex-1">{card(s, from + i)}</div>
        </li>
      ))}
    </ol>
  );

  return (
    <>
      <BackButton />
      <PageHeader title={seriesName(series, i18n.language)} />
      <div className="-mt-3 mb-8 grid max-w-prose gap-3 select-text">
        {/* Wraps after the authors as a whole when they're many. */}
        <p className="flex flex-wrap items-center gap-x-1.5 text-sm text-muted">
          {authors.length > 0 && (
            <>
              <span>
                <Authors authors={authors} links={authorLinks} />
              </span>
              <span aria-hidden>·</span>
            </>
          )}
          <span className="inline-flex items-center gap-1.5">
            {played > 0 && <Check className="size-4 text-accent" aria-hidden />}
            {t("series.played", { played, count: states.length })}
          </span>
        </p>
        {description && <p className="text-sm leading-relaxed">{description}</p>}
      </div>
      {launch.error && (
        <div className="mb-6">
          <Notice tone="error">{errorMessage(t, launch.error)}</Notice>
        </div>
      )}
      {dialogs}
      <section aria-labelledby="series-episodes" className="mb-8">
        <h2 id="series-episodes" className="mb-3 font-semibold">
          {t("series.episodes")}
        </h2>
        {list(episodes, true, 0)}
      </section>
      {bonus.length > 0 && (
        <section aria-labelledby="series-bonus">
          <h2 id="series-bonus" className="mb-3 font-semibold">
            {t("series.bonusParts")}
          </h2>
          {list(bonus, false, episodes.length)}
        </section>
      )}
    </>
  );
}

/** Back to where the series was opened from: Discover, a card or a part's page. */
function BackButton() {
  const { t } = useTranslation();
  const router = useRouter();
  return (
    <button
      type="button"
      onClick={() =>
        router.history.canGoBack() ? router.history.back() : router.navigate({ to: "/discover" })
      }
      className="mb-2 inline-flex cursor-pointer items-center gap-1 text-sm text-muted hover:text-ink"
    >
      <ArrowLeft className="size-4" aria-hidden />
      {t("series.back")}
    </button>
  );
}

/**
 * A mod not in the library, shaped like the library's cards: its game's
 * cover, what it runs on and the way to install it. It has no page of its
 * own until it's installed.
 */
function ModPartCard({
  state,
  mod,
  indexMods,
  installs,
  games,
}: {
  state: PartState;
  mod: IndexMod;
  indexMods: IndexMod[];
  installs: Install[];
  games: Game[];
}) {
  const { t } = useTranslation();
  const { entry } = mod;
  const game = games.find((g) => g.id === entry.game);
  // The mod it's played over comes along when missing.
  const base = entry.extends ? indexMods.find((m) => m.entry.id === entry.extends) : undefined;
  const size =
    (entry.downloads[0]?.size ?? 0) +
    (base && base.modId === null ? (base.entry.downloads[0]?.size ?? 0) : 0);
  return (
    <Card className="flex h-full flex-col">
      <Cover
        gameId={entry.game}
        game={game}
        standalone={isSourceMod(entry)}
        className={LARGE_COVER}
      />
      <div className="flex flex-1 flex-col gap-2 p-4">
        <div className="grid gap-0.5">
          <h3 className="text-base leading-tight font-semibold">{state.title}</h3>
          <p className="text-xs text-muted">
            {isSourceMod(entry)
              ? t("mods.standalone")
              : needsNoGame(entry)
                ? `${gameTitle(games, entry.game)} · ${t("mods.standalone")}`
                : gameTitle(games, entry.game)}
          </p>
        </div>
        <div className="flex flex-wrap gap-1">
          <Badge>{t(`discover.kind.${entry.type}`)}</Badge>
          <StatusBadge status={entry.status} />
          {entry.worlds && <Badge>{t("mods.randomWorlds")}</Badge>}
        </div>
        <div className="mt-auto grid grid-cols-1 gap-2 pt-1.5">
          <InstallAction mod={mod} installs={installs} games={games} size={size} enter={null} />
        </div>
      </div>
    </Card>
  );
}

/** One of id Software's episodes not in the library: its cover and the way to get it. */
function GamePartCard({ state, game }: { state: PartState; game?: Game }) {
  const { t } = useTranslation();
  const gameId = state.part.game ?? "";
  // The shareware episode the setup offers, if it isn't in the library.
  const free = useFreeGames().data?.some((f) => f.gameId === gameId && f.setup && !f.installed);
  return (
    <Card className="flex h-full flex-col">
      <Cover gameId={gameId} game={game} className={LARGE_COVER} />
      <div className="flex flex-1 flex-col gap-2 p-4">
        <h3 className="text-base leading-tight font-semibold">{state.title}</h3>
        <p className="text-xs text-muted">{t("series.gameMissing")}</p>
        <div className="mt-auto grid grid-cols-1 gap-2 pt-1.5">
          {free ? (
            <Link to="/discover" search={{ tab: "games" }} className={buttonClass("card")}>
              <Download className="size-4" aria-hidden />
              {t("mods.getFreeGame")}
            </Link>
          ) : (
            <Link to="/" className={buttonClass("card")}>
              <LibraryBig className="size-4" aria-hidden />
              {t("mods.addGame")}
            </Link>
          )}
        </div>
      </div>
    </Card>
  );
}
