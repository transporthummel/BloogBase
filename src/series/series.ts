import { type IndexMod, type IndexSeries, type SeriesPart } from "../bindings";
import { useSeries } from "../discover/queries";

/** Where a mod or game stands in its series. */
export type SeriesPlace = {
  series: IndexSeries;
  part: SeriesPart;
  /** The episode's number, or "bonus" for a level pack or extra. */
  number: string | "bonus";
  /** Its position among all parts: episodes first, then bonus parts. */
  index: number;
  prev?: SeriesPart;
  next?: SeriesPart;
};

/** What a part is: `{ indexId }` for an index entry, `{ gameId }` for a game. */
export type SeriesKey = { indexId: string } | { gameId: string };

const keyOf = (k: SeriesKey) => ("indexId" in k ? `mod:${k.indexId}` : `game:${k.gameId}`);

/** The parts of a series in order: episodes, then bonus parts. */
export function seriesParts(series: IndexSeries): SeriesPart[] {
  return [...series.episodes, ...(series.bonus ?? [])];
}

/** Every part of every series by what it is; built once, looked up per card. */
export function seriesPlaces(all: IndexSeries[]): Map<string, SeriesPlace> {
  const out = new Map<string, SeriesPlace>();
  for (const series of all) {
    const parts = seriesParts(series);
    parts.forEach((part, index) => {
      const key = part.mod
        ? keyOf({ indexId: part.mod })
        : part.game
          ? keyOf({ gameId: part.game })
          : null;
      if (!key || out.has(key)) return;
      out.set(key, {
        series,
        part,
        number: index < series.episodes.length ? (part.number ?? String(index + 1)) : "bonus",
        index,
        prev: parts[index - 1],
        next: parts[index + 1],
      });
    });
  }
  return out;
}

export function seriesOf(places: Map<string, SeriesPlace>, key: SeriesKey) {
  return places.get(keyOf(key));
}

const built = new WeakMap<IndexSeries[], Map<string, SeriesPlace>>();
const none = new Map<string, SeriesPlace>();

/** The places of all parts, one map for every card until the series change. */
export function useSeriesPlaces(): Map<string, SeriesPlace> {
  const { data } = useSeries();
  if (!data) return none;
  let places = built.get(data);
  if (!places) {
    places = seriesPlaces(data);
    built.set(data, places);
  }
  return places;
}

/**
 * The place of a play profile: a mod's through its index entry (`indexMods`
 * ties the installed mod to it), a game's by its id.
 */
export function profileKey(
  profile: { modId: string | null; gameId: string },
  indexMods: IndexMod[] | undefined,
): SeriesKey | null {
  if (profile.modId === null) return { gameId: profile.gameId };
  const entry = indexMods?.find((m) => m.modId === profile.modId)?.entry.id;
  return entry ? { indexId: entry } : null;
}

/** The part's own title, else the name it has. */
export function episodeTitle(part: SeriesPart, fallbackName: string): string {
  return part.title ?? fallbackName;
}

export function seriesName(series: IndexSeries, lang: string): string {
  return series.translations?.[lang]?.name ?? series.name;
}

export function seriesDescription(series: IndexSeries, lang: string): string | null {
  return series.translations?.[lang]?.description ?? series.description ?? null;
}

/** The series' names in English and `lang`, lower case, for searches. */
export function seriesSearchText(place: SeriesPlace | undefined, lang: string): string[] {
  if (!place) return [];
  return [place.series.name, seriesName(place.series, lang)].map((s) => s.toLowerCase());
}
