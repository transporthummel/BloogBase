import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { type SeriesPlace, seriesName } from "./series";

/**
 * The series above a part's title: "THE UNIVERSE IS TOAST! · 2", or "· BONUS";
 * with the count, "· 2/3".
 */
export function SeriesLine({
  place,
  link = false,
  count = false,
  className = "",
}: {
  place: SeriesPlace;
  /** Leads to the series' page; off where the whole card is a link already. */
  link?: boolean;
  /** Says how many episodes there are too, on a details page. */
  count?: boolean;
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const name = seriesName(place.series, i18n.language);
  const bonus = place.number === "bonus";
  const episodes = place.series.episodes.length;
  const number = bonus ? t("series.bonus") : place.number;
  const shown = count && !bonus ? `${number}/${episodes}` : number;
  const spoken = bonus
    ? number
    : count
      ? t("series.partOf", { number, count: episodes })
      : t("series.part", { number });
  const text = (
    <>
      {name}
      {/* No-break spaces: the number wraps with the name's last word. */}
      <span aria-hidden>&nbsp;·&nbsp;{shown}</span>
      <span className="sr-only">, {spoken}</span>
    </>
  );
  const style = `text-xs font-semibold tracking-wide text-muted uppercase ${className}`;
  return (
    <p className={style}>
      {link ? (
        <Link
          to="/series/$seriesId"
          params={{ seriesId: place.series.id }}
          className="underline-offset-2 hover:text-accent hover:underline"
        >
          {text}
        </Link>
      ) : (
        text
      )}
    </p>
  );
}
