import type { IndexEntry } from "../bindings";

/** The entry's description in `language`, or in English without a translation. */
export function entryDescription(
  entry: Pick<IndexEntry, "description" | "translations">,
  language: string,
): string | null {
  return entry.translations?.[language]?.description ?? entry.description ?? null;
}
