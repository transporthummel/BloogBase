import { type Game, type IndexEntry, type IndexMod, type Profile } from "../bindings";

export const game = (id: string, title: string): Game => ({
  id,
  title,
  shortTitle: title.replace("Commander ", ""),
  engine: "galaxy",
  executables: [],
  requiredFiles: [],
  dataFiles: [],
  saves: [],
  stateFiles: [],
  versions: [{ id: `${id}-1.4`, label: "v1.4", files: {}, shareware: false, original: null }],
});

export const entry = (id: string, name: string, more: Partial<IndexEntry> = {}): IndexEntry => ({
  id,
  name,
  game: "keen4",
  requires: [],
  authors: ["Someone"],
  added: "2020-01-01",
  type: "episode",
  links: {},
  downloads: [{ url: "https://x/y.zip", sha256: "00", size: 1024 * 1024 }],
  recipe: { type: "episode" },
  ...more,
});

export const indexMod = (e: IndexEntry, modId: string | null = null): IndexMod => ({
  entry: e,
  modId,
  updateAvailable: false,
  new: false,
});

export const profile = (id: string, more: Partial<Profile> = {}): Profile => ({
  id,
  installId: id,
  gameId: "keen4",
  versionId: null,
  modId: null,
  label: id,
  sessionCount: 0,
  playSeconds: 0,
  lastPlayedAt: null,
  addedAt: 0,
  saveCount: 0,
  running: false,
  missingGame: null,
  missingVersions: [],
  standalone: false,
  sourceMod: false,
  saveDirs: [],
  playedOver: null,
  gameProfileId: null,
  worlds: false,
  activeWorld: null,
  modStatus: null,
  variantCount: 0,
  activeVariant: null,
  variantChosen: false,
  ...more,
});
