import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type Install,
  type Mod,
  type ModInfo,
  type Profile,
} from "../bindings";
import { DropImport } from "./DropImport";
import { importPaths } from "./importPaths";

type Payload = { type: "enter" | "drop"; paths: string[] } | { type: "over" } | { type: "leave" };
let emit: (payload: Payload) => void = () => {};
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (handler: (e: { payload: Payload }) => void) => {
      emit = (payload) => handler({ payload });
      return Promise.resolve(() => {});
    },
  }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
const navigate = vi.hoisted(() => vi.fn());
vi.mock("@tanstack/react-router", () => ({ useNavigate: () => navigate }));
vi.mock("../bindings", () => ({
  commands: {
    inspectDropped: vi.fn(),
    importGames: vi.fn(),
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    installMod: vi.fn(),
    listMods: vi.fn(),
    listProfiles: vi.fn(),
    indexMods: vi.fn(),
    listJobs: vi.fn(),
  },
}));

const keen4: Game = {
  id: "keen4",
  title: "Commander Keen 4: Secret of the Oracle",
  shortTitle: "Keen 4",
  engine: "galaxy",
  executables: ["KEEN4E.EXE"],
  requiredFiles: [],
  saves: [],
  stateFiles: [],
  versions: [],
};

const install: Install = {
  id: "keen4-1",
  gameId: "keen4",
  versionId: null,
  label: keen4.title,
  source: "folder",
  path: "/x",
  addedAt: 0,
};

const info: ModInfo = {
  name: "Aurora Bliss",
  authors: [],
  version: null,
  released: null,
  gameId: "keen4",
  recipes: [
    {
      recipe: { type: "episode" },
      origin: "",
      gameVersions: [],
      versionIds: [],
      missingFiles: [],
      baseMod: null,
      madeFor: null,
    },
  ],
  confident: true,
  fileCount: 3,
  indexId: null,
};

const aurora = { id: "aurora-bliss", name: "Aurora Bliss", profileId: "aurora-bliss" } as Mod;

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install]));
  vi.mocked(commands.listMods).mockReturnValue(ok([]));
  vi.mocked(commands.listProfiles).mockReturnValue(ok([]));
});

function renderDrop() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <DropImport />
    </QueryClientProvider>,
  );
}

test("shows a hint while dragging", async () => {
  renderDrop();
  act(() => emit({ type: "enter", paths: ["/x"] }));
  expect(screen.getByText(/Drop folders or ZIP files/)).toBeInTheDocument();
  act(() => emit({ type: "leave" }));
  expect(screen.queryByText(/Drop folders or ZIP files/)).toBeNull();
});

test("imports games, confirms mods and reports what didn't work", async () => {
  vi.mocked(commands.inspectDropped).mockImplementation((path) =>
    ok(
      path.endsWith("games")
        ? { kind: "games" as const }
        : path.endsWith("aurora.zip")
          ? { kind: "mod" as const, info }
          : { kind: "nothing" as const },
    ),
  );
  vi.mocked(commands.importGames).mockReturnValue(ok([install, { ...install, id: "keen4-2" }]));
  vi.mocked(commands.installMod).mockReturnValue(ok(aurora));
  renderDrop();
  act(() =>
    emit({ type: "drop", paths: ["C:\\Keen\\games", "C:\\Downloads\\aurora.zip", "/tmp/photos"] }),
  );

  // The mod goes through the usual confirmation first.
  await userEvent.click(await screen.findByRole("button", { name: "Install" }));
  expect(commands.importGames).toHaveBeenCalledWith("C:\\Keen\\games");
  expect(commands.installMod).toHaveBeenCalledWith(
    expect.objectContaining({ path: "C:\\Downloads\\aurora.zip", installId: "keen4-1" }),
  );

  await screen.findByRole("dialog", { name: "Added to your library" });
  expect(screen.getByText("Imported 2 games.")).toBeInTheDocument();
  expect(screen.getByText("Aurora Bliss is ready to play.")).toBeInTheDocument();
  expect(screen.getByText("photos:")).toBeInTheDocument();
  expect(
    screen.getByText("Neither a Commander Keen game nor a mod was found there."),
  ).toBeInTheDocument();
  // Several things were added, so there's no one page to open.
  expect(screen.queryByRole("button", { name: "Details & Play" })).toBeNull();
});

test("one mod dropped offers its page", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "mod" as const, info }));
  vi.mocked(commands.installMod).mockReturnValue(ok(aurora));
  renderDrop();
  act(() => emit({ type: "drop", paths: ["C:\\Downloads\\aurora.zip"] }));
  await userEvent.click(await screen.findByRole("button", { name: "Install" }));

  const open = await screen.findByRole("button", { name: "Details & Play" });
  expect(open).toHaveFocus();
  await userEvent.click(open);
  expect(navigate).toHaveBeenCalledWith({
    to: "/profiles/$profileId",
    params: { profileId: "aurora-bliss" },
  });
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("one game dropped offers its own page, not a mod's", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "games" as const }));
  vi.mocked(commands.importGames).mockReturnValue(ok([install]));
  vi.mocked(commands.listProfiles).mockReturnValue(
    ok([
      { id: "aurora-bliss", installId: "keen4-1", modId: "aurora-bliss" },
      { id: "keen4-1", installId: "keen4-1", modId: null },
    ] as Profile[]),
  );
  renderDrop();
  act(() => emit({ type: "drop", paths: ["C:\\Keen\\games"] }));
  await userEvent.click(await screen.findByRole("button", { name: "Details & Play" }));
  await vi.waitFor(() =>
    expect(navigate).toHaveBeenCalledWith({
      to: "/profiles/$profileId",
      params: { profileId: "keen4-1" },
    }),
  );
});

test("imports what the add dialog picked several of, like a drop", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "games" as const }));
  vi.mocked(commands.importGames).mockReturnValue(ok([install]));
  renderDrop();
  act(() => importPaths(["/keen/one", "/keen/two"]));
  await screen.findByRole("dialog", { name: "Added to your library" });
  expect(commands.importGames).toHaveBeenCalledWith("/keen/one");
  expect(commands.importGames).toHaveBeenCalledWith("/keen/two");
});

test("several mods count up in the title and can be skipped", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "mod" as const, info }));
  vi.mocked(commands.installMod).mockReturnValue(ok(aurora));
  renderDrop();
  act(() => emit({ type: "drop", paths: ["/a.zip", "/b.zip"] }));

  await screen.findByRole("dialog", { name: "Add a game or mod (1/2)" });
  await userEvent.click(screen.getByRole("button", { name: "Skip" }));
  await screen.findByRole("dialog", { name: "Add a game or mod (2/2)" });
  await userEvent.click(screen.getByRole("button", { name: "Install" }));
  await screen.findByRole("dialog", { name: "Added to your library" });
  expect(commands.installMod).toHaveBeenCalledTimes(1);
  expect(commands.installMod).toHaveBeenCalledWith(expect.objectContaining({ path: "/b.zip" }));
});
