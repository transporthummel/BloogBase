import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Game, type Job, type RuntimeStatus } from "../bindings";
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
      freeEpisodes: vi.fn(),
      installFreeEpisode: vi.fn(),
      scanStores: vi.fn(),
      importStoreGames: vi.fn(),
      importGames: vi.fn(),
    },
    events: { jobUpdated: event },
  };
});

const game = (id: string, title: string, versionId: string): Game => ({
  id,
  title,
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

const job = (gameId: string): Job => ({
  id: 1,
  kind: "shareware",
  subject: gameId,
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
});

test("walks through all steps, skipping each, and finishes", async () => {
  vi.mocked(commands.freeEpisodes).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.scanStores).mockResolvedValue({ status: "ok", data: [] });
  renderSetup();
  const user = userEvent.setup();

  await user.click(await screen.findByRole("button", { name: "Let's go" }));
  expect(await screen.findByText("/data")).toBeInTheDocument();
  expect(getSetupStep()).toBe("storage");
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByRole("button", { name: "Download DOSBox Staging 0.83.0" });
  await user.click(screen.getByRole("button", { name: "Skip" }));
  await screen.findByRole("heading", { name: "Free episodes" });
  await user.click(screen.getByRole("button", { name: "Next" }));
  await screen.findByText(/No Commander Keen games from Steam or GOG/);
  await user.click(screen.getByRole("button", { name: "Next" }));
  await user.click(await screen.findByRole("button", { name: "Open my library" }));

  expect(commands.completeSetup).toHaveBeenCalled();
  await vi.waitFor(() => expect(navigate).toHaveBeenCalledWith({ to: "/" }));
  expect(getSetupStep()).toBeNull();
});

test("downloads the chosen free episodes as jobs", async () => {
  setSetupStep("episodes");
  vi.mocked(commands.freeEpisodes).mockResolvedValue({
    status: "ok",
    data: [
      { gameId: "keen1", versionId: "keen1-1.31", size: 150934, installed: false },
      { gameId: "keen4", versionId: "keen4-1.4-ega", size: 631332, installed: true },
    ],
  });
  vi.mocked(commands.installFreeEpisode).mockImplementation((id) => Promise.resolve(job(id)));
  vi.mocked(commands.scanStores).mockResolvedValue({ status: "ok", data: [] });
  renderSetup();
  const user = userEvent.setup();

  expect(await screen.findByText("In your library")).toBeInTheDocument();
  await user.click(await screen.findByRole("button", { name: "Download 1 episode" }));
  expect(commands.installFreeEpisode).toHaveBeenCalledTimes(1);
  expect(commands.installFreeEpisode).toHaveBeenCalledWith("keen1");
  expect(await screen.findByRole("heading", { name: "Already own Keen?" })).toBeInTheDocument();
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
  expect(await screen.findByRole("heading", { name: "All set!" })).toBeInTheDocument();
});
