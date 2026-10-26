import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type IndexMod, type IndexStatus } from "../bindings";
import {
  getLastVersion,
  getOfferedModUpdates,
  getSeenIndexMods,
  setLastVersion,
  setSeenIndexMods,
} from "../preferences";
import { WhatsNew } from "./WhatsNew";

vi.mock("@tanstack/react-router", () => ({
  Link: (props: { to: string }) => <a href={props.to} {...props} />,
}));
vi.mock("./changelog", () => ({
  changelog: [
    { version: "0.3.0", en: ["Snow at Christmas."], de: ["Schnee zu Weihnachten."] },
    { version: "0.2.0", en: ["A news dialog."], de: ["Ein Neuigkeiten-Fenster."] },
  ],
}));
vi.mock("../bindings", () => ({
  commands: {
    appInfo: vi.fn(),
    indexStatus: vi.fn(),
    indexMods: vi.fn(),
    listGames: vi.fn(),
    listProfiles: vi.fn(),
    listJobs: vi.fn(),
    installIndexMod: vi.fn(),
  },
}));

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

// The start's check is done.
const status = { checkedAt: Date.now() / 1000, error: null } as IndexStatus;

function indexMod(id: string, name: string, modId: string | null, updateAvailable = false) {
  return {
    entry: {
      id,
      name,
      game: "keen4",
      type: "episode",
      version: "2.0",
      downloads: [{ url: `https://example.org/${id}-2.zip` }],
    },
    modId,
    updateAvailable,
  } as IndexMod;
}

function renderNews() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <WhatsNew />
    </QueryClientProvider>,
  );
  return userEvent.setup();
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(commands.appInfo).mockReturnValue(ok({ version: "0.3.0" } as never));
  vi.mocked(commands.indexStatus).mockResolvedValue(status);
  vi.mocked(commands.indexMods).mockReturnValue(ok([]));
  vi.mocked(commands.listGames).mockResolvedValue([]);
  vi.mocked(commands.listProfiles).mockReturnValue(ok([]));
  vi.mocked(commands.listJobs).mockResolvedValue([]);
});

test("shows nothing on the first start and remembers what's there", async () => {
  vi.mocked(commands.indexMods).mockReturnValue(ok([indexMod("a", "Alpha", null)]));
  renderNews();
  await vi.waitFor(() => expect(getSeenIndexMods()).toEqual(["a"]));
  expect(getLastVersion()).toBe("0.3.0");
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("lists the changes since the version run before", async () => {
  setLastVersion("0.1.0");
  const user = renderNews();
  expect(await screen.findByText("New in BloogBase 0.3.0")).toBeInTheDocument();
  expect(screen.getByText("Snow at Christmas.")).toBeInTheDocument();
  expect(screen.getByText("A news dialog.")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(getLastVersion()).toBe("0.3.0");
});

test("lists mods new in the mod list", async () => {
  setLastVersion("0.3.0");
  setSeenIndexMods(["a"]);
  vi.mocked(commands.indexMods).mockReturnValue(
    ok([indexMod("a", "Alpha", null), indexMod("b", "Beta", null), indexMod("c", "Gamma", "c")]),
  );
  const user = renderNews();
  expect(await screen.findByText("New in the mod list")).toBeInTheDocument();
  expect(screen.getByText("Beta")).toBeInTheDocument();
  // Installed already: not news.
  expect(screen.queryByText("Gamma")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Close" }));
  expect(getSeenIndexMods()).toEqual(["a", "b", "c"]);
});

test("offers an installed mod's update once", async () => {
  setLastVersion("0.3.0");
  setSeenIndexMods(["a"]);
  vi.mocked(commands.indexMods).mockReturnValue(ok([indexMod("a", "Alpha", "a", true)]));
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 1,
    kind: "mod",
    subject: "a",
    phase: "downloading",
  } as never);
  const user = renderNews();
  expect(await screen.findByText("An update for one of your mods")).toBeInTheDocument();
  expect(screen.getByText("Version 2.0 is in the BloogBase mod list.")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Update" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("a", null, true, null);
  await user.click(screen.getByRole("button", { name: "Close" }));
  expect(getOfferedModUpdates()).toEqual(["a@https://example.org/a-2.zip"]);
});

test("an entry that left the mod list is new when it comes back", async () => {
  setLastVersion("0.3.0");
  setSeenIndexMods(["a", "b"]);
  vi.mocked(commands.indexMods).mockReturnValue(ok([indexMod("a", "Alpha", null)]));
  renderNews();
  await vi.waitFor(() => expect(getSeenIndexMods()).toEqual(["a"]));
  expect(screen.queryByRole("dialog")).toBeNull();

  cleanup();
  vi.mocked(commands.indexMods).mockReturnValue(
    ok([indexMod("a", "Alpha", null), indexMod("b", "Beta", null)]),
  );
  renderNews();
  expect(await screen.findByText("Beta")).toBeInTheDocument();
});

test("stays closed when nothing is new", async () => {
  setLastVersion("0.3.0");
  setSeenIndexMods(["a"]);
  vi.mocked(commands.indexMods).mockReturnValue(ok([indexMod("a", "Alpha", null)]));
  renderNews();
  await vi.waitFor(() => expect(commands.indexMods).toHaveBeenCalled());
  expect(screen.queryByRole("dialog")).toBeNull();
});
