import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type IndexEntry,
  type IndexMod,
  type Job,
  type RuntimeStatus,
} from "../bindings";
import { unwrap } from "../ipc";
import { getSetupStep, setSetupStep } from "../preferences";
import { Setup } from "./Setup";

const navigate = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }));
vi.mock("../bindings", () => {
  const event = { listen: vi.fn(() => Promise.resolve(() => {})) };
  return {
    commands: {
      appInfo: vi.fn(),
      listGames: vi.fn(),
      setupStatus: vi.fn(),
      completeSetup: vi.fn(),
      runtimeStatus: vi.fn(),
      installRuntime: vi.fn(),
      setCustomRuntime: vi.fn(),
      listJobs: vi.fn(),
      freeGames: vi.fn(),
      installFreeGame: vi.fn(),
      scanStores: vi.fn(),
      importStoreGames: vi.fn(),
      importGames: vi.fn(),
      listInstalls: vi.fn(),
      indexStatus: vi.fn(),
      indexMods: vi.fn(),
      installIndexMod: vi.fn(),
    },
    events: { jobUpdated: event, indexUpdated: event },
  };
});

const game = (id: string, title: string, versionId: string): Game => ({
  id,
  title,
  shortTitle: title,
  engine: "galaxy",
  executables: [],
  requiredFiles: [],
  saves: [],
  stateFiles: [],
  versions: [{ id: versionId, label: "v1.4 EGA", files: {}, shareware: true, start: null }],
});

const missing: RuntimeStatus = {
  active: null,
  managed: null,
  pinnedVersion: "0.83.0",
  downloadAvailable: true,
};

const job = (versionId: string): Job => ({
  id: 1,
  kind: "freeGame",
  subject: versionId,
  phase: "downloading",
  received: 0,
  total: 0,
  error: null,
});

function renderSetup() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Setup />
    </QueryClientProvider>,
  );
  return client;
}

beforeEach(() => {
  setSetupStep(null);
  navigate.mockReset();
  vi.mocked(commands.listGames).mockResolvedValue([
    game("keen1", "Commander Keen 1", "keen1-1.31"),
    game("keen4", "Commander Keen 4", "keen4-1.4-ega"),
  ]);
  vi.mocked(commands.runtimeStatus).mockResolvedValue({ status: "ok", data: missing });
  vi.mocked(commands.listJobs).mockResolvedValue([]);
  vi.mocked(commands.setupStatus).mockResolvedValue({
    status: "ok",
    data: { completed: false, hasInstalls: false },
  });
  vi.mocked(commands.appInfo).mockResolvedValue({
    status: "ok",
    data: { version: "0.1.0", dataDir: "/data", defaultDataDir: "/data", schemaVersion: 2 },
  });
  vi.mocked(commands.completeSetup).mockResolvedValue({ status: "ok", data: null });
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.indexStatus).mockResolvedValue({
    url: "x",
    checkedAt: 1,
    generatedAt: 1,
    modCount: 0,
    hiddenCount: 0,
    error: null,
  });
  vi.mocked(commands.indexMods).mockResolvedValue({ status: "ok", data: [] });
});

const indexMod = (id: string, gameId: string, requires: string[]): IndexMod => {
  const entry: IndexEntry = {
    id,
    name: id,
    game: gameId,
    requires,
    authors: ["someone"],
    added: "2026-01-01",
    type: "episode",
    links: {},
    downloads: [{ url: "https://x/y.zip", sha256: "00", size: 1 }],
    recipe: { type: "episode" },
    featured: true,
  };
  return { entry, modId: null, updateAvailable: false };
};

test("walks through all steps, skipping each, and finishes", async () => {
  vi.mocked(commands.freeGames).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.scanStores).mockResolvedValue({ status: "ok", data: [] });
  renderSetup();
  const user = userEvent.setup();

  await user.click(await screen.findByRole("button", { name: "Let's go" }));
  expect(await screen.findByText("/data")).toBeInTheDocument();
  expect(getSetupStep()).toBe("storage");
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("button", { name: "Download DOSBox Staging 0.83.0" });
  await user.click(screen.getByRole("button", { name: "Skip" }));
  await screen.findByText(/No Commander Keen games from Steam or GOG/);
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("heading", { name: "Free episodes" });
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("heading", { name: "Try a mod" });
  await user.click(screen.getByRole("button", { name: "Next" }));
  await user.click(await screen.findByRole("button", { name: "Open my library" }));

  expect(commands.completeSetup).toHaveBeenCalled();
  await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/" }));
  expect(getSetupStep()).toBeNull();
});

test("skipping setup refreshes the cached status before leaving", async () => {
  const client = renderSetup();
  // Fetched by the library before it redirected here; no observer now.
  await client.prefetchQuery({
    queryKey: ["setupStatus"],
    queryFn: () => unwrap(commands.setupStatus()),
  });
  vi.mocked(commands.setupStatus).mockResolvedValue({
    status: "ok",
    data: { completed: true, hasInstalls: false },
  });
  const user = userEvent.setup();

  await user.click(await screen.findByRole("button", { name: "Skip setup" }));

  await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/" }));
  expect(client.getQueryData(["setupStatus"])).toEqual({ completed: true, hasInstalls: false });
});

test("downloads the chosen free episodes as jobs", async () => {
  setSetupStep("episodes");
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen1", versionId: "keen1-1.31", setup: true, size: 150934, installed: false },
      { gameId: "keen4", versionId: "keen4-1.4-ega", setup: true, size: 631332, installed: true },
      // Only on the Discover page.
      { gameId: "keen4", versionId: "keen4-1.4-cga", setup: false, size: 437372, installed: false },
    ],
  });
  vi.mocked(commands.installFreeGame).mockImplementation((id) => Promise.resolve(job(id)));
  vi.mocked(commands.scanStores).mockResolvedValue({ status: "ok", data: [] });
  renderSetup();
  const user = userEvent.setup();

  expect(await screen.findByText("In your library")).toBeInTheDocument();
  await user.click(await screen.findByRole("button", { name: "Download 1 episode" }));
  expect(commands.installFreeGame).toHaveBeenCalledTimes(1);
  expect(commands.installFreeGame).toHaveBeenCalledWith("keen1-1.31");
  expect(await screen.findByRole("heading", { name: "Try a mod" })).toBeInTheDocument();
});

test("doesn't preselect free episodes whose full version is in the library", async () => {
  setSetupStep("episodes");
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen1", versionId: "keen1-1.31", setup: true, size: 150934, installed: false },
      { gameId: "keen4", versionId: "keen4-1.4-ega", setup: true, size: 631332, installed: false },
    ],
  });
  vi.mocked(commands.listInstalls).mockResolvedValue({
    status: "ok",
    data: [
      {
        id: "i1",
        gameId: "keen4",
        versionId: null,
        label: "Steam",
        source: "steam",
        path: "C:/Steam",
        addedAt: 0,
      },
    ],
  });
  vi.mocked(commands.installFreeGame).mockImplementation((id) => Promise.resolve(job(id)));
  renderSetup();
  const user = userEvent.setup();

  expect(await screen.findByText("Full version in your library")).toBeInTheDocument();
  await user.click(await screen.findByRole("button", { name: "Download 1 episode" }));
  expect(commands.installFreeGame).toHaveBeenCalledTimes(1);
  expect(commands.installFreeGame).toHaveBeenCalledWith("keen1-1.31");
});

test("still preselects a free episode when only its demo is in the library", async () => {
  setSetupStep("episodes");
  const keen4 = game("keen4", "Commander Keen 4", "keen4-1.4-ega");
  keen4.versions.push({
    id: "keen4-demo",
    label: "Special demo",
    files: {},
    shareware: false,
    kind: "demo",
  });
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen4", versionId: "keen4-1.4-ega", setup: true, size: 631332, installed: false },
    ],
  });
  vi.mocked(commands.listInstalls).mockResolvedValue({
    status: "ok",
    data: [
      {
        id: "i1",
        gameId: "keen4",
        versionId: "keen4-demo",
        label: "Demo",
        source: "folder",
        path: "C:/Demo",
        addedAt: 0,
      },
    ],
  });
  renderSetup();

  expect(await screen.findByRole("button", { name: "Download 1 episode" })).toBeEnabled();
  expect(screen.queryByText("Full version in your library")).not.toBeInTheDocument();
});

test("imports the games found in Steam and GOG folders", async () => {
  setSetupStep("owned");
  vi.mocked(commands.scanStores).mockResolvedValue({
    status: "ok",
    data: [
      {
        store: "steam",
        path: "C:/Steam/steamapps/common/Commander Keen",
        games: [
          { gameId: "keen1", versionId: null, imported: true },
          { gameId: "keen4", versionId: null, imported: false },
        ],
      },
    ],
  });
  vi.mocked(commands.importStoreGames).mockResolvedValue({ status: "ok", data: [] });
  renderSetup();
  const user = userEvent.setup();

  await user.click(await screen.findByRole("button", { name: "Add 1 game" }));
  expect(commands.importStoreGames).toHaveBeenCalledWith(
    "steam",
    "C:/Steam/steamapps/common/Commander Keen",
    ["keen4"],
  );
  expect(await screen.findByRole("heading", { name: "Free episodes" })).toBeInTheDocument();
});

test("suggests featured mods for games on their way and queues the picked ones", async () => {
  setSetupStep("mods");
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [{ gameId: "keen1", versionId: "keen1-1.31", setup: true, size: 1, installed: false }],
  });
  vi.mocked(commands.listJobs).mockResolvedValue([job("keen1-1.31")]);
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      indexMod("for-keen1", "keen1", ["keen1-1.31"]),
      indexMod("for-keen4", "keen4", []),
      {
        ...indexMod("not-featured", "keen1", []),
        entry: {
          ...indexMod("x", "keen1", []).entry,
          id: "not-featured",
          name: "not-featured",
          featured: false,
        },
      },
    ],
  });
  vi.mocked(commands.installIndexMod).mockResolvedValue({ ...job("for-keen1"), kind: "mod" });
  renderSetup();
  const user = userEvent.setup();

  await user.click(await screen.findByRole("checkbox", { name: /for-keen1/ }));
  expect(screen.queryByText("for-keen4")).not.toBeInTheDocument();
  expect(screen.queryByText("not-featured")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Install 1 mod" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("for-keen1", null, false, null);
  expect(await screen.findByRole("heading", { name: "All set!" })).toBeInTheDocument();
});
