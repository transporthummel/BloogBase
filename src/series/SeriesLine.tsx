import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { type SeriesPlace, seriesName } from "./series";

/** The series above a part's title: "THE UNIVERSE IS TOAST! · 2", or "· BONUS". */
export function SeriesLine({
  place,
  link = false,
  className = "",
}: {
  place: SeriesPlace;
  /** Leads to the series' page; off where the whole card is a link already. */
  link?: boolean;
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const name = seriesName(place.series, i18n.language);
  const number = place.number === "bonus" ? t("series.bonus") : place.number;
  const text = (
    <>
      {name}
      <span aria-hidden> · {number}</span>
      <span className="sr-only">
        , {place.number === "bonus" ? number : t("series.part", { number })}
      </span>
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
