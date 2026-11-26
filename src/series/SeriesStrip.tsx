import { Link } from "@tanstack/react-router";
import { Check, ChevronLeft, ChevronRight } from "lucide-react";
import { type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useGames } from "../catalog";
import { useIndexMods } from "../discover/queries";
import { useProfiles } from "../library/queries";
import { partStates, type PartState, type SeriesPlace, seriesName } from "./series";

/**
 * The series a part belongs to, below its credits: which part of how many,
 * the parts before and after, and every part as a chip.
 */
export function SeriesStrip({ place }: { place: SeriesPlace }) {
  const { t, i18n } = useTranslation();
  const indexMods = useIndexMods().data ?? [];
  const profiles = useProfiles().data ?? [];
  const games = useGames().data;
  const { series } = place;
  const name = seriesName(series, i18n.language);
  const states = partStates(series, indexMods, profiles, games);
  const prev = states[place.index - 1];
  const next = states[place.index + 1];
  const episodes = states.slice(0, series.episodes.length);
  const bonus = states.slice(series.episodes.length);
  const which =
    place.number === "bonus"
      ? t("series.bonus")
      : t("series.partOf", { number: place.number, count: series.episodes.length });

  const step = "inline-flex size-7 items-center justify-center rounded-md text-muted";
  return (
    <section
      aria-label={t("series.parts", { name })}
      className="grid max-w-prose gap-2 rounded-lg border border-border p-3 select-none"
    >
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <p className="mr-auto text-sm">
          <span className="text-muted">{which} · </span>
          <span className="font-semibold">{name}</span>
        </p>
        <div className="flex items-center gap-1">
          {prev ? (
            <PartLink
              state={prev}
              seriesId={series.id}
              className={`${step} hover:bg-surface-raised hover:text-ink`}
              title={`${t("series.previous")}: ${prev.title}`}
            >
              <ChevronLeft className="size-4" aria-hidden />
              <span className="sr-only">
                {t("series.previous")}: {prev.title}
              </span>
            </PartLink>
          ) : (
            <span className={`${step} opacity-40`} aria-hidden>
              <ChevronLeft className="size-4" />
            </span>
          )}
          {next ? (
            <PartLink
              state={next}
              seriesId={series.id}
              className={`${step} hover:bg-surface-raised hover:text-ink`}
              title={`${t("series.next")}: ${next.title}`}
            >
              <ChevronRight className="size-4" aria-hidden />
              <span className="sr-only">
                {t("series.next")}: {next.title}
              </span>
            </PartLink>
          ) : (
            <span className={`${step} opacity-40`} aria-hidden>
              <ChevronRight className="size-4" />
            </span>
          )}
          <Link
            to="/series/$seriesId"
            params={{ seriesId: series.id }}
            className="ml-1 text-sm text-muted underline-offset-2 hover:text-accent hover:underline"
          >
            {t("series.whole")}
          </Link>
        </div>
      </div>
      <ol className="flex flex-wrap items-center gap-1.5">
        {episodes.map((s) => (
          <Chip key={s.key} state={s} seriesId={series.id} current={s.part === place.part} />
        ))}
        {bonus.length > 0 && <li aria-hidden className="mx-1 h-5 w-px bg-border" />}
        {bonus.map((s) => (
          <Chip key={s.key} state={s} seriesId={series.id} current={s.part === place.part} />
        ))}
      </ol>
    </section>
  );
}

/** Opens the part's play profile; one the library doesn't have, on the series' page. */
function PartLink({
  state,
  seriesId,
  className,
  title,
  children,
}: {
  state: PartState;
  seriesId: string;
  className: string;
  title?: string;
  children: ReactNode;
}) {
  return state.profileId ? (
    <Link
      to="/profiles/$profileId"
      params={{ profileId: state.profileId }}
      className={className}
      title={title}
    >
      {children}
    </Link>
  ) : (
    <Link
      to="/series/$seriesId"
      params={{ seriesId }}
      hash={`part-${state.key}`}
      className={className}
      title={title}
    >
      {children}
    </Link>
  );
}

function Chip({
  state,
  seriesId,
  current,
}: {
  state: PartState;
  seriesId: string;
  current: boolean;
}) {
  const { t } = useTranslation();
  const chip = "inline-flex max-w-56 items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs";
  const content = (
    <>
      {state.number !== "bonus" && (
        <span className="font-pixel text-[0.55rem] text-muted">{state.number}</span>
      )}
      <span className="truncate">{state.title}</span>
      {state.played && (
        <>
          <Check className="size-3 shrink-0 text-accent" aria-hidden />
          <span className="sr-only">({t("series.playedMark")})</span>
        </>
      )}
    </>
  );
  return (
    <li className="min-w-0">
      {current ? (
        <span aria-current="page" className={`${chip} border-accent bg-accent/10 font-semibold`}>
          {content}
        </span>
      ) : (
        <PartLink
          state={state}
          seriesId={seriesId}
          className={`${chip} border-border hover:border-accent/50 hover:text-ink ${
            state.profileId ? "" : "text-muted"
          }`}
          title={state.profileId ? undefined : t("series.notInstalled")}
        >
          {content}
        </PartLink>
      )}
    </li>
  );
}
