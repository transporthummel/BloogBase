/**
 * What changed in each BloogBase release, newest first. The news dialog shows
 * the entries newer than the version a device ran before. Add one before
 * tagging a release (docs/releasing.md).
 */
export type Release = { version: string; en: string[]; de: string[] };

export const changelog: Release[] = [
  {
    version: "0.1.0",
    en: ["The first release of BloogBase."],
    de: ["Die erste Version von BloogBase."],
  },
];
