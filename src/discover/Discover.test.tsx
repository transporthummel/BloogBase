import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
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
import { compare, fitsEntry } from "./fit";

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
    dataFiles: [],
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
    dataFiles: [],
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
  new: false,
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

// The last rendered page's queries.
let client: QueryClient;

function renderDiscover() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <Discover />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  search = {};
  vi.mocked(commands.listGames).mockResolvedValue(games);
  vi.mocked(commands.indexStatus).mockResolvedValue(status);
  vi.mocked(commands.freeGames).mockResolvedValue({ status: "ok", data: [] });
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

/** A choice of the filter `group`, e.g. "Game". */
function choice(group: string, name: string) {
  return within(screen.getByRole("group", { name: group })).getByRole("button", { name });
}

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

test("marks unfinished mods, abandoned ones as a warning", async () => {
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(entry("Aurora", { status: "beta" })),
      mod(entry("Krodacia", { status: "abandoned" })),
      mod(entry("Oblivion", { status: "complete" })),
      mod(entry("Fribbulus", { status: "gamma" })),
    ],
  });
  renderDiscover();

  await screen.findByRole("heading", { name: "Aurora" });
  expect(within(card("Aurora")).getByText("Beta")).toBeInTheDocument();
  expect(within(card("Krodacia")).getByText("Abandoned")).toHaveClass("text-warning");
  for (const name of ["Oblivion", "Fribbulus"]) {
    expect(within(card(name)).queryByText(/^(complete|gamma)$/i)).not.toBeInTheDocument();
  }
});

test("names the copy of another version and explains why it won't do", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({
    status: "ok",
    data: [install("keen6-a", "keen6", "keen6-1.0")],
  });
  renderDiscover();
  const user = userEvent.setup();

  await screen.findByRole("heading", { name: "Fribbulus" });
  const fribbulus = within(card("Fribbulus"));
  expect(fribbulus.getByRole("status")).toHaveTextContent(
    "Needs Commander Keen 6 (v1.4 EGA). You have v1.0. Learn more",
  );
  await user.click(fribbulus.getByRole("button", { name: "Learn more" }));
  expect(screen.getByRole("dialog", { name: "Why this exact version?" })).toHaveTextContent(
    "Unrecognised version",
  );
});

test("installs the free shareware along with a mod made for it", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen4", versionId: "keen4-1.4-ega", setup: true, size: 600000, installed: false },
      { gameId: "keen4", versionId: "keen4-demo", setup: false, size: 538631, installed: false },
    ],
  });
  vi.mocked(commands.installFreeGame).mockResolvedValue({
    id: 4,
    kind: "freeGame",
    subject: "keen4-1.4-ega",
    phase: "downloading",
    received: 0,
    total: 0,
    error: null,
  });
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 5,
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
  const aurora = within(card("Aurora"));
  expect(await aurora.findByText("With Keen 4 v1.4 EGA (shareware)")).toBeInTheDocument();
  expect(aurora.queryByRole("status")).not.toBeInTheDocument();
  // Keen 6 has no free version.
  expect(within(card("Fribbulus")).getByRole("status")).toHaveTextContent("Needs Commander Keen 6");
  // Playable: the shareware comes along.
  await user.click(screen.getByRole("button", { name: "I can play" }));
  expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual([
    "Aurora",
    "Krodacia",
    "Oblivion",
  ]);
  await user.click(aurora.getByRole("button", { name: "Install (2.6 MB)" }));
  expect(commands.installFreeGame).toHaveBeenCalledWith("keen4-1.4-ega");
  // Without a copy yet: the mod's job waits for the shareware and goes on it.
  expect(commands.installIndexMod).toHaveBeenCalledWith("Aurora", null, true, null);
  // Discover keeps its filters; the next test starts without.
  await user.click(screen.getByRole("button", { name: "I can play" }));
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
  // The count says how many of them are shown.
  expect(screen.getByText(/^1 of 4 mods · checked /)).toBeInTheDocument();
  await user.clear(screen.getByRole("searchbox"));
  expect(screen.getByText(/^4 mods · checked /)).toBeInTheDocument();

  const names = () => screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
  await user.click(choice("Type", "Level pack"));
  expect(names()).toEqual(["Krodacia"]);
  expect(choice("Type", "All")).toHaveAttribute("aria-pressed", "false");
  // Several types and games show the mods of any of them.
  await user.click(choice("Type", "Adventure"));
  expect(names()).toHaveLength(4);
  await user.click(choice("Type", "Level pack"));
  expect(names()).toEqual(["Aurora", "Fribbulus", "Oblivion"]);
  await user.click(choice("Type", "All"));
  expect(choice("Type", "Adventure")).toHaveAttribute("aria-pressed", "false");

  await user.click(choice("Game", "Commander Keen 6"));
  expect(names()).toEqual(["Fribbulus"]);
  await user.click(choice("Game", "Commander Keen 4"));
  expect(names()).toHaveLength(4);
  await user.click(choice("Game", "Commander Keen 6"));
  expect(names()).toEqual(["Aurora", "Krodacia", "Oblivion"]);
  await user.click(choice("Game", "Commander Keen 4"));
  expect(choice("Game", "All")).toHaveAttribute("aria-pressed", "true");

  const playable = screen.getByRole("button", { name: "I can play" });
  expect(playable).toHaveAccessibleDescription(/for one of your games/i);
  await user.click(playable);
  expect(screen.queryByRole("heading", { name: "Fribbulus" })).not.toBeInTheDocument();
  expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(3);
  await user.click(screen.getByRole("button", { name: "I can play" }));
});

test("clears the filters once one is set", async () => {
  renderDiscover();
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: "Aurora" });
  expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();

  await user.click(choice("Game", "Commander Keen 6"));
  await user.type(screen.getByRole("searchbox"), "nothing like it");
  expect(screen.getByText("No mods match these filters.")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Clear filters" }));
  expect(screen.getAllByRole("heading", { level: 2 })).toHaveLength(4);
  expect(choice("Game", "All")).toHaveAttribute("aria-pressed", "true");
  expect(screen.queryByRole("button", { name: "Clear filters" })).not.toBeInTheDocument();
});

test("hides the mods installed when the filter was turned on", async () => {
  renderDiscover();
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: "Aurora" });
  const filter = () => screen.getByRole("button", { name: "Not installed" });
  const names = () => screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);

  await user.click(filter());
  expect(names()).toEqual(["Aurora", "Fribbulus"]);

  // One installed meanwhile stays until the filter is turned on again.
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(entry("Aurora", { description: "Blissful", tags: ["sequel"] }), { modId: "aurora" }),
      mod(entry("Krodacia", { type: "levelpack" }), { modId: "krodacia" }),
      mod(entry("Oblivion", { version: "1.4" }), { modId: "oblivion", updateAvailable: true }),
      mod(entry("Fribbulus", { game: "keen6", requires: ["keen6-1.4-ega"] })),
    ],
  });
  await client.invalidateQueries();
  await within(card("Aurora")).findByText("Installed");
  expect(names()).toEqual(["Aurora", "Fribbulus"]);

  await user.click(filter());
  expect(names()).toHaveLength(4);
  await user.click(filter());
  expect(names()).toEqual(["Fribbulus"]);
  await user.click(filter());
});

test("standalone mods are under no game and install on none", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(entry("Aurora")),
      mod(
        entry("Foray", {
          requires: [],
          recipe: { type: "program", program: "FITF.EXE", args: [] },
          standalone: true,
          // A source mod's downloads list its programs.
          downloads: [
            {
              url: "https://x/y.zip",
              sha256: "00",
              size: 2 * 1024 * 1024,
              programs: { "FITF.EXE": "ab" },
            },
          ],
        }),
      ),
    ],
  });
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 3,
    kind: "mod",
    subject: "Foray",
    phase: "downloading",
    received: 0,
    total: 0,
    error: null,
  });
  renderDiscover();
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: "Foray" });

  const foray = within(card("Foray"));
  expect(foray.getByText("Standalone")).toBeInTheDocument();
  expect(foray.getByText("Source mod")).toBeInTheDocument();
  const heading = () => screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent);
  await user.click(screen.getByRole("button", { name: "I can play" }));
  expect(heading()).toEqual(["Foray"]);
  await user.click(screen.getByRole("button", { name: "I can play" }));
  await user.click(choice("Game", "Commander Keen 4"));
  expect(heading()).toEqual(["Aurora"]);
  await user.click(choice("Game", "Commander Keen 4"));
  await user.click(choice("Game", "Standalone"));
  expect(heading()).toEqual(["Foray"]);

  await user.click(within(card("Foray")).getByRole("button", { name: "Install (2.0 MB)" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(commands.installIndexMod).toHaveBeenCalledWith("Foray", null, true, null);
  await user.click(choice("Game", "All"));
});

test("source mods that require their game are listed like other mods", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      mod(
        entry("Pack", {
          requires: [],
          recipe: { type: "program", program: "RCK4.EXE", args: [] },
          standalone: true,
          requiresGame: true,
        }),
      ),
    ],
  });
  renderDiscover();
  const user = userEvent.setup();
  await screen.findByRole("heading", { name: "Pack" });

  const pack = within(card("Pack"));
  expect(pack.queryByText("Standalone")).not.toBeInTheDocument();
  expect(pack.queryByText("Source mod")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Standalone" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "I can play" }));
  expect(screen.queryByRole("heading", { name: "Pack" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "I can play" }));
  await user.click(choice("Game", "Commander Keen 4"));
  expect(screen.getByRole("heading", { name: "Pack" })).toBeInTheDocument();
  await user.click(choice("Game", "All"));
});

/** The choices of the mods tab, all set to something else than the defaults. */
async function chooseAll(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByRole("searchbox"), "a");
  await user.click(choice("Game", "Commander Keen 4"));
  await user.click(choice("Type", "Adventure"));
  await user.selectOptions(screen.getByRole("combobox", { name: "Sort by" }), "name");
  await user.click(screen.getByRole("button", { name: "I can play" }));
  await user.click(screen.getByRole("button", { name: "Not installed" }));
}

function expectAllChosen() {
  expect(screen.getByRole("searchbox")).toHaveValue("a");
  expect(choice("Game", "Commander Keen 4")).toHaveAttribute("aria-pressed", "true");
  expect(choice("Type", "Adventure")).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("name");
  expect(screen.getByRole("button", { name: "I can play" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(screen.getByRole("button", { name: "Not installed" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
}

async function resetChoices(user: ReturnType<typeof userEvent.setup>) {
  await user.clear(screen.getByRole("searchbox"));
  await user.click(choice("Game", "All"));
  await user.click(choice("Type", "All"));
  await user.selectOptions(screen.getByRole("combobox", { name: "Sort by" }), "featured");
  const playable = screen.getByRole("button", { name: "I can play" });
  if (playable.getAttribute("aria-pressed") === "true") await user.click(playable);
  const installed = screen.getByRole("button", { name: "Not installed" });
  if (installed.getAttribute("aria-pressed") === "true") await user.click(installed);
}

test("keeps filters and sort across other pages and tabs, but not in storage", async () => {
  const user = userEvent.setup();
  const first = renderDiscover();
  await screen.findByRole("heading", { name: "Aurora" });
  await chooseAll(user);
  first.unmount();

  renderDiscover();
  await screen.findByRole("heading", { name: "Aurora" });
  expectAllChosen();
  expect(JSON.stringify(localStorage)).not.toContain("keen4");
  expect(JSON.stringify(sessionStorage)).not.toContain("keen4");

  await user.click(screen.getByRole("tab", { name: "Games" }));
  await user.click(screen.getByRole("tab", { name: "Mods" }));
  await screen.findByRole("heading", { name: "Aurora" });
  expectAllChosen();
  await resetChoices(user);
});

test("a link to the newest mods clears remembered filters once", async () => {
  const user = userEvent.setup();
  const first = renderDiscover();
  await screen.findByRole("heading", { name: "Aurora" });
  await chooseAll(user);
  first.unmount();

  search = { tab: "mods", sort: "newest" };
  renderDiscover();
  await screen.findByRole("heading", { name: "Fribbulus" });
  expect(screen.getByRole("searchbox")).toHaveValue("");
  expect(choice("Game", "All")).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("newest");

  // Going to another tab and back isn't following the link again.
  await user.selectOptions(screen.getByRole("combobox", { name: "Sort by" }), "name");
  await user.click(screen.getByRole("tab", { name: "Games" }));
  await user.click(screen.getByRole("tab", { name: "Mods" }));
  await screen.findByRole("heading", { name: "Fribbulus" });
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("name");

  // Leaving the page and following the link again is.
  cleanup();
  renderDiscover();
  await screen.findByRole("heading", { name: "Fribbulus" });
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("newest");
  await resetChoices(user);
});

test("opens sorted by newest when linked that way", async () => {
  search = { tab: "mods", sort: "newest" };
  renderDiscover();
  await screen.findByRole("heading", { name: "Aurora" });
  expect(screen.getByRole("combobox", { name: "Sort by" })).toHaveValue("newest");
  await userEvent.selectOptions(screen.getByRole("combobox", { name: "Sort by" }), "featured");
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

test("sorting", () => {
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
        missingVersions: [],
        standalone: false,
        sourceMod: false,
        saveDirs: [],
        playedOver: null,
        gameProfileId: null,
        worlds: false,
        activeWorld: null,
        modStatus: null,
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
  await user.click(within(cga).getByRole("button", { name: "Install (428 KB)" }));
  expect(commands.installFreeGame).toHaveBeenCalledWith("keen4-1.4-cga");

  await user.click(screen.getByRole("tab", { name: "Demos & Betas" }));
  expect(versions()).toEqual(["Special demo"]);
  // The version says it's a demo; no second badge for that.
  expect(screen.queryByText("Demo")).toBeNull();
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
