import { Link } from "@tanstack/react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useGames } from "../catalog";
import { useIndexMods } from "../discover/queries";
import { useProfiles } from "../library/queries";
import { partStates, type PartState, type SeriesPlace } from "./series";

/**
 * The parts before and after this one, as arrows beside the series' line on a
 * details page. At either end the arrow stays, dimmed, so the other one keeps
 * its place while stepping through.
 */
export function SeriesSteps({ place }: { place: SeriesPlace }) {
  const { t } = useTranslation();
  const indexMods = useIndexMods().data ?? [];
  const profiles = useProfiles().data ?? [];
  const games = useGames().data;
  const states = partStates(place.series, indexMods, profiles, games);
  return (
    // As tall as the line of text it sits in; the targets reach past it.
    <div className="-my-1.5 flex shrink-0 items-center gap-0.5 select-none">
      <Step
        state={states[place.index - 1]}
        seriesId={place.series.id}
        label={t("series.previous")}
        icon={ChevronLeft}
      />
      <Step
        state={states[place.index + 1]}
        seriesId={place.series.id}
        label={t("series.next")}
        icon={ChevronRight}
      />
    </div>
  );
}

/** Opens the part's play profile; one the library doesn't have, its card on the series' page. */
function Step({
  state,
  seriesId,
  label,
  icon: Icon,
}: {
  state?: PartState;
  seriesId: string;
  label: string;
  icon: typeof ChevronLeft;
}) {
  const { t } = useTranslation();
  const step = "inline-flex size-7 items-center justify-center rounded-md text-muted";
  if (!state) {
    return (
      <span className={`${step} opacity-40`} aria-hidden>
        <Icon className="size-4" />
      </span>
    );
  }
  const name = state.profileId
    ? `${label}: ${state.title}`
    : `${label}: ${state.title} (${t("series.notInstalled")})`;
  const content = (
    <>
      <Icon className="size-4" aria-hidden />
      <span className="sr-only">{name}</span>
    </>
  );
  const style = `${step} hover:bg-surface-raised hover:text-ink`;
  return state.profileId ? (
    <Link
      to="/profiles/$profileId"
      params={{ profileId: state.profileId }}
      className={style}
      title={name}
    >
      {content}
    </Link>
  ) : (
    <Link
      to="/series/$seriesId"
      params={{ seriesId }}
      hash={`part-${state.key}`}
      className={style}
      title={name}
    >
      {content}
    </Link>
  );
}
