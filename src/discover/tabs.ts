export const TABS = ["mods", "games", "demos"] as const;
export type DiscoverTab = (typeof TABS)[number];
export const isDiscoverTab = (v: unknown): v is DiscoverTab => TABS.includes(v as DiscoverTab);
