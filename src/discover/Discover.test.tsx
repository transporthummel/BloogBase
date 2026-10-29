import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type IndexEntry,
  type IndexMod,
  type IndexStatus,
  type Install,
} from "../bindings";
import { Discover } from "./Discover";
import { compare, fitsEntry, isNew } from "./fit";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
// What the page was opened with.
let search: { tab?: string; sort?: string } = {};

vi.mock("@tanstack/react-router", () => ({
  Link: ({
    to,
    params,
    children,
  }: {
    to: string;
    params?: Record<string, string>;
    children: React.ReactNode;
  }) => (
    <a href={Object.entries(params ?? {}).reduce((p, [k, v]) => p.replace(`$${k}`, v), to)}>
      {children}
    </a>
  ),
  useSearch: () => search,
}));
vi.mock("../bindings", () => ({
  commands: {
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    listJobs: vi.fn(() => Promise.resolve([])),
    indexStatus: vi.fn(),
    indexMods: vi.fn(),
    refreshIndex: vi.fn(),
    installIndexMod: vi.fn(),
    freeGames: vi.fn(),
    installFreeGame: vi.fn(),
    listProfiles: vi.fn(),
  },
}));

const version = (id: string, label: string, original: string | null = null) => ({
  id,
  label,
  files: {},
  shareware: false,
  original,
});

const games: Game[] = [
  {
    id: "keen4",
    title: "Commander Keen 4",
    shortTitle: "Keen 4",
    engine: "galaxy",
    executables: [],
    requiredFiles: [],
    saves: [],
    stateFiles: [],
    versions: [
      { ...version("keen4-1.4-ega", "v1.4 EGA"), shareware: true },
      { ...version("keen4-1.4-cga", "v1.4 CGA"), shareware: true },
      { ...version("keen4-demo", "Special demo"), kind: "demo" },
    ],
  },
  {
    id: "keen6",
    title: "Commander Keen 6",
    shortTitle: "Keen 6",
    engine: "galaxy",
    executables: [],
    requiredFiles: [],
    saves: [],
    stateFiles: [],
    versions: [
      version("keen6-1.0", "v1.0"),
      version("keen6-1.0-nocp", "v1.0 (no copy protection)", "keen6-1.0"),
      version("keen6-1.4-ega", "v1.4 EGA"),
    ],
  },
];

const install = (id: string, gameId: string, versionId: string | null): Install => ({
  id,
  gameId,
  versionId,
  label: id,
  source: "folder",
  path: "/x",
  addedAt: 0,
});

const entry = (id: string, more: Partial<IndexEntry> = {}): IndexEntry => ({
  id,
  name: id,
  game: "keen4",
  requires: ["keen4-1.4-ega"],
  authors: ["Someone"],
  added: "2020-01-01",
  type: "episode",
  links: {},
  downloads: [{ url: "https://x/y.zip", sha256: "00", size: 2 * 1024 * 1024 }],
  recipe: { type: "episode" },
  ...more,
});

const mod = (e: IndexEntry, more: Partial<IndexMod> = {}): IndexMod => ({
  entry: e,
  modId: null,
  updateAvailable: false,
  ...more,
});

const status: IndexStatus = {
  url: "x",
  checkedAt: 1_760_000_000,
  generatedAt: 1,
  modCount: 4,
  hiddenCount: 0,
  error: null,
};

function renderDiscover() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Discover />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  search = {};
  vi.mocked(commands.listGames).mockResolvedValue(games);
  vi.mocked(commands.indexStatus).mockResolvedValue(status);
  vi.mocked(commands.listInstalls).mockResolvedValue({
    status: "ok",
    data: [install("keen4-a", "keen4", "keen4-1.4-ega"), install("keen4-b", "keen4", null)],
  });
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(entry("Aurora", { description: "Blissful", tags: ["sequel"] })),
      mod(entry("Krodacia", { type: "levelpack" }), { modId: "krodacia" }),
      mod(entry("Oblivion", { version: "1.4" }), { modId: "oblivion", updateAvailable: true }),
      mod(entry("Fribbulus", { game: "keen6", requires: ["keen6-1.4-ega"] })),
    ],
  });
});

function card(name: string) {
  return screen.getByRole("heading", { name }).closest("article") as HTMLElement;
}

test("shows what each mod needs and installs on the chosen game", async () => {
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 1,
    kind: "mod",
    subject: "Aurora",
    phase: "downloading",
    received: 0,
    total: 0,
    error: null,
  });
  renderDiscover();
  const user = userEvent.setup();

  await screen.findByRole("heading", { name: "Aurora" });
  expect(screen.getByText(/^4 mods · checked \d+ years? ago$/)).toBeInTheDocument();
  expect(within(card("Krodacia")).getByText("Details & Play")).toHaveAttribute(
    "href",
    "/profiles/krodacia",
  );
  expect(
    within(card("Oblivion")).getByRole("button", { name: "Update to version 1.4" }),
  ).toBeInTheDocument();
  expect(
    within(card("Fribbulus")).getByText("Needs Commander Keen 6 (v1.4 EGA)."),
  ).toBeInTheDocument();

  const aurora = within(card("Aurora"));
  await user.selectOptions(aurora.getByRole("combobox"), "keen4-b");
  await user.click(aurora.getByRole("button", { name: "Install (2.0 MB)" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("Aurora", "keen4-b", false, null);
});

test("asks before installing a mod without its game", async () => {
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 2,
    kind: "mod",
    subject: "Fribbulus",
    phase: "downloading",
    received: 0,
    total: 0,
    error: null,
  });
  renderDiscover();
  const user = userEvent.setup();

  await screen.findByRole("heading", { name: "Fribbulus" });
  await user.click(within(card("Fribbulus")).getByRole("button", { name: "Install anyway" }));
  const dialog = screen.getByRole("dialog", { name: "Install Fribbulus without its game?" });
  expect(dialog).toHaveTextContent("Commander Keen 6 (v1.4 EGA) isn’t in your library.");
  expect(commands.installIndexMod).not.toHaveBeenCalled();
  await user.click(within(dialog).getByRole("button", { name: "Install (2.0 MB)" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("Fribbulus", null, true, null);
});

test("filters by text, type and what the library can play", async () => {
  renderDiscover();
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: "Aurora" });

  await user.type(screen.getByRole("searchbox"), "sequel");
  expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual([
    "Aurora",
  ]);
  await user.clear(screen.getByRole("searchbox"));

  await user.selectOptions(screen.getByRole("combobox", { name: "Type" }), "levelpack");
  expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(1);
  await user.selectOptions(screen.getByRole("combobox", { name: "Type" }), "");

  await user.click(screen.getByRole("checkbox", { name: "Works with my games" }));
  expect(screen.queryByRole("heading", { name: "Fribbulus" })).not.toBeInTheDocument();
  expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(3);
});

test("opens sorted by newest when linked that way", async () => {
  search = { tab: "mods", sort: "newest" };
  renderDiscover();
  await screen.findByRole("heading", { name: "Aurora" });
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("newest");
});

test("keeps showing the cached list when a check fails", async () => {
  vi.mocked(commands.indexStatus).mockResolvedValue({
    ...status,
    hiddenCount: 2,
    error: { kind: "network", message: "offline" },
  });
  renderDiscover();
  expect(await screen.findByText(/couldn't check for new mods/)).toBeInTheDocument();
  expect(screen.getByText("Update BloogBase to see 2 more mods.")).toBeInTheDocument();
  expect(await screen.findByRole("heading", { name: "Aurora" })).toBeInTheDocument();

  vi.mocked(commands.refreshIndex).mockResolvedValue(status);
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(commands.refreshIndex).toHaveBeenCalledWith(true);
  await vi.waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
});

test("says which download failed when there's no list yet", async () => {
  vi.mocked(commands.indexStatus).mockResolvedValue({
    ...status,
    checkedAt: null,
    modCount: 0,
    error: { kind: "network", message: "offline" },
  });
  vi.mocked(commands.indexMods).mockResolvedValue({ status: "ok", data: [] });
  renderDiscover();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "BloogBase couldn't download the mod list.",
  );
  expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
});

test("fit rules match the backend", () => {
  const k6 = games[1];
  const e = entry("x", { game: "keen6", requires: ["keen6-1.0"] });
  expect(fitsEntry(e, { gameId: "keen6", versionId: "keen6-1.0-nocp" }, k6)).toBe(true);
  expect(fitsEntry(e, { gameId: "keen6", versionId: "keen6-1.4-ega" }, k6)).toBe(false);
  expect(fitsEntry(e, { gameId: "keen6", versionId: null }, k6)).toBe(true);
  expect(fitsEntry(e, { gameId: "keen4", versionId: null }, k6)).toBe(false);
  expect(
    fitsEntry({ ...e, requires: [] }, { gameId: "keen6", versionId: "keen6-1.4-ega" }, k6),
  ).toBe(true);
});

test("new badges and sorting", () => {
  const now = Date.parse("2026-10-17T12:00:00Z");
  const first = entry("first", { added: "2026-07-01" });
  const recent = entry("a", { added: "2026-10-01" });
  expect(isNew(recent, [first, recent], now)).toBe(true);
  expect(isNew(entry("a", { added: "2026-08-01" }), [first], now)).toBe(false);
  // While the index is in its first month, nothing in it is new.
  expect(isNew(recent, [entry("b", { added: "2026-09-20" }), recent], now)).toBe(false);
  const list = [
    entry("b", { added: "2026-01-01" }),
    entry("a", { added: "2025-01-01", featured: true }),
    entry("c", { added: "2026-05-01" }),
  ];
  const ids = (sort: Parameters<typeof compare>[0]) =>
    [...list].sort(compare(sort)).map((e) => e.id);
  expect(ids("featured")).toEqual(["a", "c", "b"]);
  expect(ids("newest")).toEqual(["c", "b", "a"]);
  expect(ids("name")).toEqual(["a", "b", "c"]);
});

test("offers shareware episodes and demos on their own tabs", async () => {
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen4", versionId: "keen4-1.4-ega", setup: true, size: 1, installed: true },
      { gameId: "keen4", versionId: "keen4-1.4-cga", setup: false, size: 437372, installed: false },
      { gameId: "keen4", versionId: "keen4-demo", setup: false, size: 538631, installed: false },
    ],
  });
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [
      {
        id: "p-ega",
        installId: "keen4-a",
        gameId: "keen4",
        versionId: "keen4-1.4-ega",
        modId: null,
        label: "Keen 4",
        sessionCount: 0,
        playSeconds: 0,
        lastPlayedAt: null,
        addedAt: 0,
        saveCount: 0,
        running: false,
        missingGame: null,
        playedOver: null,
      },
    ],
  });
  vi.mocked(commands.installFreeGame).mockResolvedValue({
    id: 3,
    kind: "freeGame",
    subject: "keen4-1.4-cga",
    phase: "downloading",
    received: 0,
    total: 0,
    error: null,
  });
  renderDiscover();
  const user = userEvent.setup();

  await user.click(await screen.findByRole("tab", { name: "Games" }));
  const versions = () =>
    screen.getAllByRole("article").map((a) => within(a).getByText(/^v1|Special/).textContent);
  expect(await screen.findAllByRole("article")).toHaveLength(2);
  expect(versions()).toEqual(["v1.4 EGA", "v1.4 CGA"]);
  expect(screen.queryByRole("button", { name: "Check for new mods" })).not.toBeInTheDocument();
  const [ega, cga] = screen.getAllByRole("article");
  expect(await within(ega).findByText("Details & Play")).toHaveAttribute("href", "/profiles/p-ega");
  await user.click(within(cga).getByRole("button", { name: "Download (0.4 MB)" }));
  expect(commands.installFreeGame).toHaveBeenCalledWith("keen4-1.4-cga");

  await user.click(screen.getByRole("tab", { name: "Demos & Betas" }));
  expect(versions()).toEqual(["Special demo"]);
  expect(screen.getByText("Demo")).toBeInTheDocument();
});

test("a mod played over another says so and counts its download too", async () => {
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(entry("Aurora")),
      mod(entry("Pack", { type: "levelpack", extends: "Aurora", requires: [] })),
    ],
  });
  renderDiscover();

  await screen.findByRole("heading", { name: "Pack" });
  const pack = within(card("Pack"));
  expect(pack.getByText("Played over Aurora, which is installed with it")).toBeInTheDocument();
  expect(pack.getByRole("button", { name: "Install (4.0 MB)" })).toBeInTheDocument();
});
