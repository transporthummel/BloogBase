import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type Install,
  type Mod,
  type Profile,
  type RuntimeStatus,
} from "../bindings";
import { Library } from "./Library";
import { profilesKey } from "./queries";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}` }));
const navigate = vi.hoisted(() => vi.fn());
vi.mock("@tanstack/react-router", () => ({
  // Real links have an href, which makes them focusable.
  Link: (props: { to: string }) => <a href={props.to} {...props} />,
  Navigate: ({ to }: { to: string }) => <p>redirect to {to}</p>,
  useNavigate: () => navigate,
}));
// Stands in for picking: one button per outcome.
vi.mock("../mods/AddModDialog", () => ({
  AddModDialog: (props: {
    onInstalled: (mod: Mod) => void;
    onGamesImported: (installs: Install[]) => void;
  }) => (
    <>
      <button onClick={() => props.onInstalled({ profileId: "aurora" } as Mod)}>mod</button>
      <button onClick={() => props.onGamesImported([{ id: "keen5-1" } as Install])}>
        one game
      </button>
      <button
        onClick={() => props.onGamesImported([{ id: "keen5-1" }, { id: "keen5-2" }] as Install[])}
      >
        two games
      </button>
    </>
  ),
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
      storageUsage: vi.fn(),
      profileCover: vi.fn(() => Promise.resolve({ status: "ok", data: null })),
    },
    events: { playSessionEnded: event, jobUpdated: event },
  };
});

const keen5: Game = {
  id: "keen5",
  title: "Commander Keen 5: The Armageddon Machine",
  shortTitle: "Keen 5",
  engine: "galaxy",
  executables: ["KEEN5E.EXE"],
  requiredFiles: [],
  saves: [],
  stateFiles: [],
  versions: [
    { id: "keen5-1.4-ega", label: "v1.4 EGA", files: {}, shareware: false, original: null },
  ],
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
  addedAt: 1_690_000_000,
  saveCount: 1,
  running: false,
  missingGame: null,
};

const runtime = { id: "dosbox-staging-0.83.0", version: "0.83.0", path: "/x", managed: true };
const ready: RuntimeStatus = {
  active: runtime,
  managed: runtime,
  pinnedVersion: "0.83.0",
  downloadAvailable: true,
};

function renderLibrary(container?: HTMLElement) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <Library />
    </QueryClientProvider>,
    // A fresh root each time, inside the given element.
    { container: container?.appendChild(document.createElement("div")) },
  );
  return Object.assign(client, { unmount: view.unmount });
}

beforeEach(() => {
  navigate.mockClear();
  vi.mocked(commands.storageUsage).mockResolvedValue({
    status: "ok",
    data: {
      games: 2e6,
      mods: 5e5,
      profiles: 1e4,
      backups: 0,
      runtimes: 4e7,
      cache: 0,
      total: 42_600_000,
    },
  });
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

test("shows the empty state with ways to add games", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [] });
  renderLibrary();
  expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Add game or mod" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Get free games" })).toHaveAttribute("href", "/discover");
});

test("lists profiles with version and play stats", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  renderLibrary();
  expect(await screen.findByText(keen5.title)).toBeInTheDocument();
  expect(await screen.findByText("v1.4 EGA")).toBeInTheDocument();
  expect(screen.getByText("Played 1 h 5 min")).toBeInTheDocument();
  // On the card and in the status bar.
  expect(screen.getAllByText("1 saved game")).toHaveLength(2);
});

test("coming back from a details page keeps the place and shows the card", async () => {
  const other = { ...profile, id: "other", label: "Other Game" };
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile, other] });
  const scrolled = vi.fn();
  Element.prototype.scrollIntoView = scrolled;
  // Pages scroll inside the shell's <main>.
  const main = document.body.appendChild(document.createElement("main"));
  const first = renderLibrary(main);
  main.scrollTop = 300;
  await userEvent.click(await screen.findByRole("link", { name: "Other Game" }));
  first.unmount();

  main.scrollTop = 0;
  renderLibrary(main);
  const link = await screen.findByRole("link", { name: "Other Game" });
  await vi.waitFor(() => expect(link).toHaveFocus());
  expect(main.scrollTop).toBe(300);
  expect(scrolled).toHaveBeenCalledTimes(1);
  expect(scrolled.mock.contexts[0]).toHaveAttribute("data-profile-id", "other");
  main.remove();
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

test("a mod waiting for its game can't be played", async () => {
  const waiting: Profile = {
    ...profile,
    id: "galactic",
    installId: null,
    versionId: null,
    modId: "galactic",
    label: "Galactic",
    missingGame: "Commander Keen 5 (v1.4 EGA)",
  };
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [waiting] });
  renderLibrary();
  expect(await screen.findByText("Needs Commander Keen 5 (v1.4 EGA)")).toBeInTheDocument();
  const play = screen.getByRole("button", { name: "Play Galactic" });
  expect(play).toHaveTextContent("Game missing");
  expect(play).toBeDisabled();
});

test("a mod gets its title screen once its game is there", async () => {
  const waiting: Profile = { ...profile, id: "galactic", installId: null, modId: "galactic" };
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [waiting] });
  const client = renderLibrary();
  await vi.waitFor(() => expect(commands.profileCover).toHaveBeenCalledTimes(1));

  vi.mocked(commands.profileCover).mockResolvedValue({
    status: "ok",
    data: "/covers/galactic.png",
  });
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [{ ...waiting, installId: "keen5-1" }],
  });
  await client.invalidateQueries({ queryKey: profilesKey });
  await vi.waitFor(() =>
    expect(document.querySelector('img[src^="asset:///covers/galactic.png?v="]')).not.toBeNull(),
  );
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

test("sorting by type groups games and mods and is remembered", async () => {
  const mod: Profile = { ...profile, id: "aurora", modId: "aurora", label: "Aurora Bliss" };
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [mod, profile] });
  renderLibrary();
  await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Sort" }), "type");
  const games = screen.getByRole("region", { name: "Games" });
  expect(within(games).getByRole("heading", { name: keen5.title })).toBeInTheDocument();
  const mods = screen.getByRole("region", { name: "Mods" });
  expect(within(mods).getByRole("heading", { name: "Aurora Bliss" })).toBeInTheDocument();
  expect(localStorage.getItem("bloogbase.librarySort")).toBe("type");
  localStorage.removeItem("bloogbase.librarySort");
});

test("the status bar sums up the library and its disk space", async () => {
  const mod: Profile = { ...profile, id: "aurora", modId: "aurora", label: "Aurora Bliss" };
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [mod, profile] });
  renderLibrary();
  const bar = await screen.findByRole("contentinfo");
  expect(within(bar).getByText("1 game")).toBeInTheDocument();
  expect(within(bar).getByText("1 mod")).toBeInTheDocument();
  expect(within(bar).getByText("2 h 10 min played")).toBeInTheDocument();
  expect(within(bar).getByText("2 saved games")).toBeInTheDocument();
  expect(await within(bar).findByText(/42.6 MB used/)).toHaveAttribute(
    "title",
    expect.stringContaining("DOSBox: 40 MB"),
  );
});

test.each([
  ["mod", "aurora"],
  ["one game", "keen5-1"],
])("adding a single %s opens its page", async (pick, profileId) => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  renderLibrary();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add game or mod" }));
  await user.click(screen.getByRole("button", { name: pick }));
  await vi.waitFor(() =>
    expect(navigate).toHaveBeenCalledWith({ to: "/profiles/$profileId", params: { profileId } }),
  );
});

test("adding several games stays in the library and counts them", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
  renderLibrary();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Add game or mod" }));
  await user.click(screen.getByRole("button", { name: "two games" }));
  expect(await screen.findByText("Imported 2 games.")).toBeInTheDocument();
  expect(navigate).not.toHaveBeenCalled();
});
