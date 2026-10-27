import news from "./changelog.json";

/**
 * What changed in each BloogBase release, newest first, from
 * `changelog.json` (bundled, so it works offline). The news dialog shows the
 * entries newer than the version a device ran before. New entries go under
 * "unreleased" with their commit; releasing moves them under the version
 * (docs/releasing.md).
 */
export type Release = { version: string; en: string[]; de: string[] };

export const changelog: Release[] = news.releases;
