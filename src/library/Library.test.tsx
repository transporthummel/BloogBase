import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Game, type Install } from "../bindings";
import { Library } from "./Library";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: { listGames: vi.fn(), listInstalls: vi.fn(), importGames: vi.fn() },
}));

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

const install: Install = {
  id: "keen5-1",
  gameId: "keen5",
  versionId: "keen5-1.4-ega",
  label: keen5.title,
  source: "folder",
  path: "/data/library/keen5-1",
  addedAt: 0,
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
});

test("shows the empty state with import actions", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  renderLibrary();
  expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Import folder" })).toBeInTheDocument();
});

test("lists installs with their version", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [install] });
  renderLibrary();
  expect(await screen.findByText(keen5.title)).toBeInTheDocument();
  expect(await screen.findByText("v1.4 EGA")).toBeInTheDocument();
});

test("shows a translated error", async () => {
  vi.mocked(commands.listInstalls).mockResolvedValue({
    status: "error",
    error: { kind: "noGamesFound", message: "x" },
  });
  renderLibrary();
  expect(await screen.findByRole("alert")).toHaveTextContent("No Commander Keen games");
});
