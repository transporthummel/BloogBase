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
vi.mock("@tanstack/react-router", () => ({
  Link: ({ to, children }: { to: string; children: React.ReactNode }) => (
    <a href={to}>{children}</a>
  ),
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
    engine: "galaxy",
    executables: [],
    requiredFiles: [],
    saves: [],
    stateFiles: [],
    versions: [version("keen4-1.4-ega", "v1.4 EGA")],
  },
  {
    id: "keen6",
    title: "Commander Keen 6",
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
  expect(screen.getByText("4 mods · checked Oct 9, 2025")).toBeInTheDocument();
  expect(within(card("Krodacia")).getByText("Play it in your library")).toBeInTheDocument();
  expect(
    within(card("Oblivion")).getByRole("button", { name: "Update to version 1.4" }),
  ).toBeInTheDocument();
  expect(
    within(card("Fribbulus")).getByText("Needs Commander Keen 6 (v1.4 EGA)."),
  ).toBeInTheDocument();

  const aurora = within(card("Aurora"));
  await user.selectOptions(aurora.getByRole("combobox"), "keen4-b");
  await user.click(aurora.getByRole("button", { name: "Install (2.0 MB)" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("Aurora", "keen4-b");
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

test("keeps showing the cached list when a check fails", async () => {
  vi.mocked(commands.indexStatus).mockResolvedValue({
    ...status,
    hiddenCount: 2,
    error: { kind: "network", message: "offline" },
  });
  renderDiscover();
  expect(await screen.findByText(/from your last visit/)).toBeInTheDocument();
  expect(screen.getByText("Update BloogBase to see 2 more mods.")).toBeInTheDocument();
  expect(await screen.findByRole("heading", { name: "Aurora" })).toBeInTheDocument();
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
  expect(isNew(entry("a", { added: "2026-10-01" }), now)).toBe(true);
  expect(isNew(entry("a", { added: "2026-08-01" }), now)).toBe(false);
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
