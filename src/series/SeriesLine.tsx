import { useTranslation } from "react-i18next";
import { type SeriesPlace, seriesName } from "./series";

/** The series above a part's title: "THE UNIVERSE IS TOAST! · 2", or "· BONUS". */
export function SeriesLine({ place, className = "" }: { place: SeriesPlace; className?: string }) {
  const { t, i18n } = useTranslation();
  const name = seriesName(place.series, i18n.language);
  const number = place.number === "bonus" ? t("series.bonus") : place.number;
  return (
    <p className={`text-xs font-semibold tracking-wide text-muted uppercase ${className}`}>
      {name}
      <span aria-hidden> · {number}</span>
      <span className="sr-only">
        , {place.number === "bonus" ? number : t("series.part", { number })}
      </span>
    </p>
  );
}
