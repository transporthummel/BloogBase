import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Game, type Profile, type RuntimeStatus } from "../bindings";
import { Library } from "./Library";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({
  Link: (props: object) => <a {...props} />,
  Navigate: ({ to }: { to: string }) => <p>redirect to {to}</p>,
}));
vi.mock("../bindings", () => {
  const event = { listen: vi.fn(() => Promise.resolve(() => {})) };
  return {
    commands: {
      listGames: vi.fn(),
      listProfiles: vi.fn(),
      importGames: vi.fn(),
      launchProfile: vi.fn(),
      runtimeStatus: vi.fn(),
      installRuntime: vi.fn(),
      listJobs: vi.fn(() => Promise.resolve([])),
      setupStatus: vi.fn(),
    },
    events: { playSessionEnded: event, jobUpdated: event },
  };
});

const keen5: Game = {
  id: "keen5",
  title: "Commander Keen 5: The Armageddon Machine",
  engine: "galaxy",
  executables: ["KEEN5E.EXE"],
  requiredFiles: [],
  saves: [],
  stateFiles: [],
  versions: [{ id: "keen5-1.4-ega", label: "v1.4 EGA", files: {}, shareware: false }],
};

const profile: Profile = {
  id: "keen5-1",
  installId: "keen5-1",
  gameId: "keen5",
  versionId: "keen5-1.4-ega",
  modId: null,
  label: keen5.title,
  sessionCount: 2,
  playSeconds: 3900,
  lastPlayedAt: 1_700_000_000,
  saveCount: 1,
  running: false,
};

const runtime = { id: "dosbox-staging-0.83.0", version: "0.83.0", path: "/x", managed: true };
const ready: RuntimeStatus = {
  active: runtime,
  managed: runtime,
  pinnedVersion: "0.83.0",
  downloadAvailable: true,
};

function renderLibrary() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Library />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(commands.listGames).mockResolvedValue([keen5]);
  vi.mocked(commands.runtimeStatus).mockResolvedValue({ status: "ok", data: ready });
  vi.mocked(commands.setupStatus).mockResolvedValue({
    status: "ok",
    data: { completed: true, hasInstalls: true },
  });
});

test("opens the setup wizard on first start", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.setupStatus).mockResolvedValue({
    status: "ok",
    data: { completed: false, hasInstalls: false },
  });
  renderLibrary();
  expect(await screen.findByText("redirect to /setup")).toBeInTheDocument();
});

test("shows the empty state with import actions", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [] });
  renderLibrary();
  expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Import folder" })).toBeInTheDocument();
});

test("lists profiles with version and play stats", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  renderLibrary();
  expect(await screen.findByText(keen5.title)).toBeInTheDocument();
  expect(await screen.findByText("v1.4 EGA")).toBeInTheDocument();
  expect(screen.getByText("Played 1 h 5 min")).toBeInTheDocument();
  expect(screen.getByText("1 saved game")).toBeInTheDocument();
});

test("play launches the profile", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  vi.mocked(commands.launchProfile).mockResolvedValue({ status: "ok", data: null });
  renderLibrary();
  const play = await screen.findByRole("button", { name: `Play ${keen5.title}` });
  await vi.waitFor(() => expect(play).toBeEnabled());
  await userEvent.click(play);
  expect(commands.launchProfile).toHaveBeenCalledWith("keen5-1");
});

test("asks for DOSBox when no runtime is installed", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  vi.mocked(commands.runtimeStatus).mockResolvedValue({
    status: "ok",
    data: { ...ready, active: null, managed: null },
  });
  renderLibrary();
  expect(
    await screen.findByRole("button", { name: "Download DOSBox Staging 0.83.0" }),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: `Play ${keen5.title}` })).toBeDisabled();
});

test("shows a translated error", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "error",
    error: { kind: "noGamesFound", message: "x" },
  });
  renderLibrary();
  expect(await screen.findByRole("alert")).toHaveTextContent("No Commander Keen games");
});
