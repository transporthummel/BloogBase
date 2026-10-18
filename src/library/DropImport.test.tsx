import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Game, type Install, type Mod, type ModInfo } from "../bindings";
import { DropImport } from "./DropImport";

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
vi.mock("../bindings", () => ({
  commands: {
    inspectDropped: vi.fn(),
    importGames: vi.fn(),
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    installMod: vi.fn(),
    indexMods: vi.fn(),
    listJobs: vi.fn(),
  },
}));

const keen4: Game = {
  id: "keen4",
  title: "Commander Keen 4: Secret of the Oracle",
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
  recipes: [{ recipe: { type: "episode" }, origin: "", gameVersions: [], versionIds: [] }],
  confident: true,
  fileCount: 3,
};

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install]));
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
  vi.mocked(commands.installMod).mockReturnValue(ok({ name: "Aurora Bliss" } as Mod));
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
});
