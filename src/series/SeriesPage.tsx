import { Link, useParams, useRouter, useRouterState } from "@tanstack/react-router";
import { ArrowLeft, Check, Download, LibraryBig } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { type Game, type Profile } from "../bindings";
import { DEVELOPER, DEVELOPER_URL, useGames } from "../catalog";
import { DetailsLink } from "../discover/DetailsLink";
import { ModCard } from "../discover/ModCard";
import { useFreeGames, useIndexMods, useSeries } from "../discover/queries";
import { Cover } from "../library/Cover";
import { useCover } from "../library/covers";
import { useInstalls, useProfiles } from "../library/queries";
import { Authors } from "../mods/Authors";
import { buttonClass } from "../ui/buttonClass";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Loading } from "../ui/Loading";
import { PageHeader } from "../ui/PageHeader";
import { partStates, type PartState, seriesDescription, seriesName } from "./series";

export function SeriesRoute() {
  const { seriesId } = useParams({ from: "/series/$seriesId" });
  return <SeriesPage key={seriesId} seriesId={seriesId} />;
}

/** A whole series: its parts in order, each to install or play. */
export function SeriesPage({ seriesId }: { seriesId: string }) {
  const { t, i18n } = useTranslation();
  const all = useSeries();
  const indexMods = useIndexMods();
  const profiles = useProfiles();
  const installs = useInstalls();
  const games = useGames().data ?? [];
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

  const list = (parts: PartState[], numbered: boolean) => (
    <ol className="grid grid-cols-[repeat(auto-fill,minmax(18rem,1fr))] gap-4">
      {parts.map((s) => (
        <li key={s.key} id={`part-${s.key}`} className="flex scroll-mt-6 gap-3">
          {numbered && (
            <span
              aria-hidden
              className="w-8 shrink-0 pt-4 text-right font-pixel text-sm text-muted"
            >
              {s.number}
            </span>
          )}
          <div className="min-w-0 flex-1">
            {s.indexMod ? (
              <ModCard
                mod={s.indexMod}
                installs={installs.data ?? []}
                games={games}
                heading="h3"
                seriesLine={false}
              />
            ) : (
              <GameCard
                state={s}
                game={games.find((g) => g.id === s.part.game)}
                profiles={profiles.data ?? []}
              />
            )}
          </div>
        </li>
      ))}
    </ol>
  );

  return (
    <>
      <BackButton />
      <PageHeader title={seriesName(series, i18n.language)} />
      <div className="-mt-3 mb-8 grid max-w-prose gap-3 select-text">
        <div className="grid gap-0.5 text-sm text-muted">
          {authors.length > 0 && (
            <p>
              <Authors authors={authors} links={authorLinks} />
            </p>
          )}
          <p className="flex items-center gap-1.5">
            {played > 0 && <Check className="size-4 text-accent" aria-hidden />}
            {t("series.played", { played, count: states.length })}
          </p>
        </div>
        {description && <p className="text-sm leading-relaxed">{description}</p>}
      </div>
      <section aria-labelledby="series-episodes" className="mb-8">
        <h2 id="series-episodes" className="mb-3 font-semibold">
          {t("series.episodes")}
        </h2>
        {list(episodes, true)}
      </section>
      {bonus.length > 0 && (
        <section aria-labelledby="series-bonus">
          <h2 id="series-bonus" className="mb-3 font-semibold">
            {t("series.bonusParts")}
          </h2>
          {list(bonus, false)}
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

/** One of id Software's episodes: its cover and the way to play it, or to get it. */
function GameCard({
  state,
  game,
  profiles,
}: {
  state: PartState;
  game?: Game;
  profiles: Profile[];
}) {
  const { t } = useTranslation();
  const gameId = state.part.game ?? "";
  const profile = profiles.find((p) => p.id === state.profileId);
  // The shareware episode the setup offers, if it isn't in the library.
  const free = useFreeGames().data?.some((f) => f.gameId === gameId && f.setup && !f.installed);
  return (
    <Card className="flex h-full flex-col">
      {profile ? (
        <ProfileCover profile={profile} game={game} />
      ) : (
        <Cover gameId={gameId} game={game} />
      )}
      <div className="flex flex-1 flex-col gap-3 p-4">
        <h3 className="leading-tight font-semibold">{state.title}</h3>
        <div className="mt-auto grid grid-cols-1 gap-2 pt-1">
          {state.profileId ? (
            <DetailsLink profileId={state.profileId} />
          ) : (
            <>
              <p className="text-sm text-muted">{t("series.gameMissing")}</p>
              {free ? (
                <Link to="/discover" search={{ tab: "games" }} className={buttonClass("secondary")}>
                  <Download className="size-4" aria-hidden />
                  {t("mods.getFreeGame")}
                </Link>
              ) : (
                <Link to="/" className={buttonClass("secondary")}>
                  <LibraryBig className="size-4" aria-hidden />
                  {t("mods.addGame")}
                </Link>
              )}
            </>
          )}
        </div>
      </div>
    </Card>
  );
}

/** The title screen of the copy it opens. */
function ProfileCover({ profile, game }: { profile: Profile; game?: Game }) {
  const cover = useCover(profile.id, profile.installId, null);
  return (
    <Cover gameId={profile.gameId} game={game} src={cover.data?.src} onError={cover.onError} />
  );
}
