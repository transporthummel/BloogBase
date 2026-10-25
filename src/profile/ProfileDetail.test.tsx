import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  type Backup,
  type Game,
  type Install,
  type Mod,
  type Profile,
  type RuntimeStatus,
  type SaveFile,
  type Screenshot,
  type Video,
} from "../bindings";
import { ProfileDetail } from "./ProfilePage";
import { slotFiles } from "./saves";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ convertFileSrc: (p: string) => `asset://${p}` }));
const navigate = vi.hoisted(() => vi.fn());
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a>{children}</a>,
  useNavigate: () => navigate,
  useParams: vi.fn(),
}));
// CodeMirror needs layout APIs jsdom lacks; a textarea stands in.
vi.mock("@uiw/react-codemirror", () => ({
  default: ({
    value,
    onChange,
    readOnly,
    "aria-label": label,
  }: {
    value: string;
    onChange?: (v: string) => void;
    readOnly?: boolean;
    "aria-label"?: string;
  }) => (
    <textarea
      aria-label={label}
      value={value}
      readOnly={readOnly}
      onChange={(e) => onChange?.(e.target.value)}
    />
  ),
}));
vi.mock("../bindings", () => {
  const event = { listen: vi.fn(() => Promise.resolve(() => {})) };
  const names = [
    "listGames",
    "listProfiles",
    "launchProfile",
    "runtimeStatus",
    "listSaves",
    "saveVersions",
    "deleteSave",
    "listBackups",
    "createBackup",
    "backupSaves",
    "restoreBackup",
    "deleteBackup",
    "exportBackup",
    "importBackup",
    "configView",
    "saveConfig",
    "resetConfig",
    "listMods",
    "modReadmes",
    "openModFolder",
    "openModReadme",
    "deleteMod",
    "modInstalls",
    "setModInstall",
    "removeGame",
    "indexMods",
    "listScreenshots",
    "profileCover",
    "deleteScreenshot",
    "listVideos",
    "deleteVideo",
    "videoIndex",
    "openCaptureFolder",
    "oneButtonFiring",
    "profileFiring",
  ];
  return {
    commands: Object.fromEntries(names.map((n) => [n, vi.fn()])),
    events: { playSessionEnded: event },
  };
});

const keen4: Game = {
  id: "keen4",
  title: "Commander Keen 4: Secret of the Oracle",
  shortTitle: "Keen 4",
  engine: "galaxy",
  executables: ["KEEN4E.EXE"],
  requiredFiles: [],
  saves: ["SAVEGAM?.CK4"],
  stateFiles: ["CONFIG.CK4"],
  versions: [],
};

const profile: Profile = {
  id: "keen4-1",
  installId: "keen4-1",
  gameId: "keen4",
  versionId: null,
  modId: null,
  label: keen4.title,
  sessionCount: 1,
  playSeconds: 60,
  lastPlayedAt: 1_700_000_000,
  addedAt: 1_690_000_000,
  saveCount: 2,
  running: false,
  missingGame: null,
};

const save = (file: string, slot: number, name: string | null): SaveFile => ({
  file,
  slot,
  modifiedAt: 1_700_000_000,
  size: 100,
  versions: 0,
  details: name
    ? {
        name,
        score: 48100,
        lives: 13,
        ammo: 45,
        levelsDone: 2,
        difficulty: "hard",
        onWorldMap: false,
        shipParts: null,
      }
    : null,
});

const backup: Backup = {
  id: 7,
  profileId: "keen4-1",
  label: null,
  reason: "beforeLaunch",
  createdAt: 1_700_000_000,
  files: ["CONFIG.CK4", "SAVEGAM0.CK4"],
  saves: 1,
  settings: 1,
  other: 0,
  auto: true,
  size: 300,
};

const ready: RuntimeStatus = {
  active: { id: "x", version: "0.83.0", path: "/x", managed: true },
  managed: null,
  pinnedVersion: "0.83.0",
  downloadAvailable: true,
};

const ok = <T,>(data: T) => ({ status: "ok" as const, data });

function renderDetail(client = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return render(
    <QueryClientProvider client={client}>
      <ProfileDetail profileId="keen4-1" />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([profile]));
  vi.mocked(commands.runtimeStatus).mockResolvedValue(ok(ready));
  vi.mocked(commands.listSaves).mockResolvedValue(
    ok([save("SAVEGAM0.CK4", 1, "pabba"), save("SAVEGAM5.CK4", 6, null)]),
  );
  vi.mocked(commands.listBackups).mockResolvedValue(ok([backup]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([]));
  vi.mocked(commands.modReadmes).mockResolvedValue(ok([]));
  vi.mocked(commands.indexMods).mockResolvedValue(ok([]));
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([]));
  vi.mocked(commands.listVideos).mockResolvedValue(ok([]));
  vi.mocked(commands.profileCover).mockResolvedValue(ok(null));
  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(false));
  vi.mocked(commands.profileFiring).mockResolvedValue(ok(null));
  vi.mocked(commands.modInstalls).mockResolvedValue(ok([]));
});

const aurora: Mod = {
  id: "aurora-bliss",
  gameId: "keen4",
  name: "Aurora Bliss",
  authors: ["Dave216", "Ceilick"],
  authorLinks: { Dave216: "https://keenwiki.shikadi.net/wiki/User:Dave216" },
  version: "1.2",
  description: "A colourful journey.",
  released: "2012-05-01",
  recipe: { type: "episode" },
  source: "keenwiki",
  indexId: null,
  keenwikiUrl: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss",
  addedAt: 1_700_000_000,
  profileId: "aurora-bliss",
};

/** Picks one of the actions in the hero's menu. */
async function pick(action: string) {
  await userEvent.click(await screen.findByRole("button", { name: "More" }));
  await userEvent.click(screen.getByRole("menuitem", { name: action }));
}

const shot = (file: string, takenAt: number): Screenshot => ({
  file,
  path: `/data/profiles/keen4-1/capture/${file}`,
  takenAt,
  size: 1000,
});

test("a mod's page shows its credits, description, links and stats", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, id: "keen4-1", modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  renderDetail();
  expect(await screen.findByRole("heading", { name: "Aurora Bliss" })).toBeInTheDocument();
  const author = await screen.findByRole("button", { name: "Dave216" });
  expect(author.parentElement).toHaveTextContent(/^by Dave216, Ceilick$/);
  // Without a page, a name is plain text.
  expect(screen.queryByRole("button", { name: "Ceilick" })).toBeNull();
  await userEvent.click(author);
  expect(openUrl).toHaveBeenCalledWith("https://keenwiki.shikadi.net/wiki/User:Dave216");
  expect(screen.getByText("A colourful journey.")).toBeInTheDocument();
  expect(screen.getByText(`Runs on ${keen4.title} Unknown version`)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "KeenWiki" }));
  expect(openUrl).toHaveBeenCalledWith(aurora.keenwikiUrl);
  // Not in the index yet.
  expect(screen.getByRole("button", { name: "Suggest it for the mod list" })).toBeInTheDocument();
  expect(screen.getByText("Sessions").nextSibling).toHaveTextContent("1");
  vi.mocked(commands.openModFolder).mockResolvedValue(ok(null));
  await pick("Open mod folder");
  expect(commands.openModFolder).toHaveBeenCalledWith("aurora-bliss");
});

test("a mod's page changes the copy of the game it plays on", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  const copy = (id: string, label: string): Install => ({
    id,
    gameId: "keen4",
    versionId: null,
    label,
    source: "folder",
    path: `/games/${id}`,
    addedAt: 1_690_000_000,
  });
  vi.mocked(commands.modInstalls).mockResolvedValue(
    ok([copy("keen4-1", "Keen 4"), copy("keen4-2", "Keen 4 (GOG)")]),
  );
  vi.mocked(commands.setModInstall).mockResolvedValue(ok(null));
  renderDetail();
  // Only offered with more than one copy.
  await vi.waitFor(() => expect(commands.modInstalls).toHaveBeenCalled());
  await pick("Play with another copy…");
  const picker = screen.getByRole("combobox", { name: "Play with" });
  expect(picker).toHaveValue("keen4-1");
  await userEvent.selectOptions(picker, "keen4-2");
  expect(commands.setModInstall).toHaveBeenCalledWith("aurora-bliss", "keen4-2");
});

test("a mod is removed from its page, keeping saves unless unticked", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.deleteMod).mockResolvedValue(ok(null));
  const client = new QueryClient();
  const remove = vi.spyOn(client, "removeQueries");
  renderDetail(client);
  await pick("Remove");
  const dialog = screen.getByRole("dialog");
  await userEvent.click(within(dialog).getByRole("checkbox"));
  await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(commands.deleteMod).toHaveBeenCalledWith("aurora-bliss", false);
  expect(navigate).toHaveBeenCalledWith({ to: "/" });
  // Installed again, it gets the same id and must not show the old cover.
  expect(remove).toHaveBeenCalledWith({ queryKey: ["cover", "keen4-1"] });
});

test("opens at the top, wherever the library was scrolled to", async () => {
  // Pages scroll inside the shell's <main>.
  const main = document.body.appendChild(document.createElement("main"));
  main.scrollTop = 500;
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ProfileDetail profileId="keen4-1" />
    </QueryClientProvider>,
    { container: main },
  );
  expect(main.scrollTop).toBe(0);
  main.remove();
});

test("a mod can be replaced with a local copy, unless it runs", async () => {
  const mine = { ...profile, modId: "aurora-bliss", label: "Aurora Bliss" };
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([mine]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  renderDetail();
  await pick("Replace with a local copy…");
  expect(screen.getByRole("dialog", { name: "Replace with another copy" })).toBeInTheDocument();

  cleanup();
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...mine, running: true }]));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "More" }));
  expect(screen.getByRole("menuitem", { name: "Replace with a local copy…" })).toBeDisabled();
});

test("a game is uninstalled from its page, with its mods if asked", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([
      profile,
      { ...profile, id: "aurora-bliss", modId: "aurora-bliss", label: "Aurora Bliss" },
      { ...profile, id: "other", installId: "keen4-2", modId: "other", label: "Other" },
    ]),
  );
  vi.mocked(commands.removeGame).mockResolvedValue(ok(null));
  const client = new QueryClient();
  const remove = vi.spyOn(client, "removeQueries");
  renderDetail(client);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "More" }));
  expect(screen.queryByRole("menuitem", { name: "Remove" })).toBeNull();
  await user.click(screen.getByRole("menuitem", { name: "Uninstall" }));
  const dialog = screen.getByRole("dialog");
  // Only the mod on this copy of the game counts.
  const withMods = within(dialog).getByRole("checkbox", {
    name: "Also uninstall the mod played on it",
  });
  expect(dialog).toHaveTextContent("Otherwise they move to another copy");
  await user.click(withMods);
  await user.click(within(dialog).getByRole("button", { name: "Uninstall" }));
  expect(commands.removeGame).toHaveBeenCalledWith("keen4-1", true, true);
  expect(navigate).toHaveBeenCalledWith({ to: "/" });
  expect(remove).toHaveBeenCalledWith({ queryKey: ["cover"] });
});

test("a mod waiting for its game says which one and can't be played", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([
      {
        ...profile,
        installId: null,
        modId: "aurora-bliss",
        label: "Aurora Bliss",
        missingGame: "Commander Keen 4 (v1.4 EGA)",
      },
    ]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  renderDetail();
  expect(await screen.findByText("Needs Commander Keen 4 (v1.4 EGA)")).toBeInTheDocument();
  expect(
    screen.getByText("Add Commander Keen 4 (v1.4 EGA) to your library to play this mod."),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Game missing" })).toBeDisabled();
  expect(screen.queryByText(/^Runs on/)).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "More" }));
  expect(screen.queryByRole("menuitem", { name: "Uninstall" })).toBeNull();

  // Removing it doesn't mention a game that stays.
  await userEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveTextContent("The mod’s files are deleted.");
  expect(dialog).not.toHaveTextContent("The game itself stays");
});

test("a mod's README is the first tab, shown as text with links opening outside", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([
      {
        file: "Aurora/READ ME.txt",
        text: '<img src=x onerror="alert(1)"><script>alert(2)</script>\nSee https://pckf.com/aurora.',
        html: false,
        word: false,
      },
    ]),
  );
  const { container } = renderDetail();
  const tab = await screen.findByRole("tab", { name: "Readme" });
  expect(tab).toHaveAttribute("aria-selected", "true");
  expect(screen.getAllByRole("tab")[0]).toBe(tab);
  expect(commands.modReadmes).toHaveBeenCalledWith("aurora-bliss");
  expect(screen.getByText("Aurora/READ ME.txt")).toBeInTheDocument();
  const panel = screen.getByRole("tabpanel");
  expect(panel).toHaveTextContent('<img src=x onerror="alert(1)"><script>alert(2)</script>');
  expect(container.querySelector("script, [onerror]")).toBeNull();
  await userEvent.click(within(panel).getByRole("button", { name: "https://pckf.com/aurora" }));
  expect(openUrl).toHaveBeenCalledWith("https://pckf.com/aurora");
});

test("other documents of a mod can be picked in the README tab", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([
      { file: "changes.txt", text: "v1.1: fixed a crash", html: false, word: false },
      { file: "Hints Sheet.txt", text: "The key is behind the wall", html: false, word: false },
    ]),
  );
  renderDetail();
  const picker = await screen.findByRole("combobox", { name: "File:" });
  expect(picker).toHaveValue("changes.txt");
  const panel = screen.getByRole("tabpanel");
  expect(panel).toHaveTextContent("v1.1: fixed a crash");
  await userEvent.selectOptions(picker, "Hints Sheet.txt");
  expect(panel).toHaveTextContent("The key is behind the wall");
  expect(panel).not.toHaveTextContent("v1.1");
});

test("a web page of a mod opens in the browser instead of the app", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([{ file: "Files/README.HTM", text: "", html: true, word: false }]),
  );
  vi.mocked(commands.openModReadme).mockResolvedValue(ok(null));
  renderDetail();
  const panel = await screen.findByRole("tabpanel");
  expect(panel).toHaveTextContent("Files/README.HTM");
  expect(panel.querySelector("pre")).toBeNull();
  await userEvent.click(within(panel).getByRole("button", { name: "Open in browser" }));
  expect(commands.openModReadme).toHaveBeenCalledWith("aurora-bliss", "Files/README.HTM");
});

test("a Word document of a mod shows its text and opens in its app", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([
      { file: "keenstatic.doc", text: "World of Static", html: false, word: true },
      { file: "hints.doc", text: "", html: false, word: true },
    ]),
  );
  vi.mocked(commands.openModReadme).mockResolvedValue(ok(null));
  renderDetail();
  const panel = await screen.findByRole("tabpanel");
  expect(panel.querySelector("pre")).toHaveTextContent("World of Static");
  expect(panel).toHaveTextContent("Only its text is shown here");
  await userEvent.click(within(panel).getByRole("button", { name: "Open in default app" }));
  expect(commands.openModReadme).toHaveBeenCalledWith("aurora-bliss", "keenstatic.doc");
  await userEvent.selectOptions(screen.getByRole("combobox", { name: "File:" }), "hints.doc");
  expect(panel.querySelector("pre")).toBeNull();
  expect(panel).toHaveTextContent("BloogBase can't read");
});

test("without a README there is no README tab", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(ok([]));
  renderDetail();
  expect(await screen.findByRole("tab", { name: "Saves" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.queryByRole("tab", { name: "Readme" })).toBeNull();
});

test("games don't look for a README", async () => {
  renderDetail();
  expect(await screen.findByRole("tab", { name: "Saves" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(commands.modReadmes).not.toHaveBeenCalled();
  expect(screen.queryByRole("button", { name: "Open mod folder" })).toBeNull();
});

test("says how Keen 1-3 shoot where the setting or the mod changes it", async () => {
  vi.mocked(commands.profileFiring).mockResolvedValue(ok("patchable"));
  renderDetail();
  await screen.findByRole("heading", { name: keen4.title });
  expect(commands.profileFiring).toHaveBeenCalledWith("keen4-1");
  expect(screen.queryByText(/Space/)).toBeNull();
  cleanup();

  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(true));
  renderDetail();
  expect(
    await screen.findByText("Space shoots, Enter opens the status window."),
  ).toBeInTheDocument();
  cleanup();

  // A mod's own one-button firing shows with the setting off too.
  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(false));
  vi.mocked(commands.profileFiring).mockResolvedValue(ok("builtIn"));
  renderDetail();
  expect(await screen.findByText("This mod already shoots with Space.")).toBeInTheDocument();
  cleanup();

  // On another version of the game, it says which one the patch is for.
  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(true));
  vi.mocked(commands.profileFiring).mockResolvedValue(ok("otherVersion"));
  renderDetail();
  expect(await screen.findByText(/only available for version 1\.31/)).toBeInTheDocument();
});

test("the title screen is the picture, else the latest screenshot", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([shot("image0001.png", 1_700_000_000)]));
  const { unmount } = renderDetail();
  expect(
    await screen.findByRole("img", { name: `Screenshot of ${profile.label}` }),
  ).toHaveAttribute("src", "asset:///data/profiles/keen4-1/capture/image0001.png");
  unmount();

  vi.mocked(commands.profileCover).mockResolvedValue(
    ok({ path: "/data/cache/covers/keen4-1.png", scrambled: false }),
  );
  const { container, unmount: unmountCover } = renderDetail();
  await vi.waitFor(() =>
    expect(container.querySelector("img")?.getAttribute("src")).toMatch(
      /^asset:\/\/\/data\/cache\/covers\/keen4-1\.png\?v=\d+$/,
    ),
  );
  expect(screen.queryByText(/graphics files don't fit together/)).toBeNull();
  unmountCover();

  // Scrambled graphics, as in Keen Static, get a warning.
  vi.mocked(commands.profileCover).mockResolvedValue(
    ok({ path: "/data/cache/covers/keen4-1.png", scrambled: true }),
  );
  renderDetail();
  expect(await screen.findByText(/graphics files don't fit together/)).toBeInTheDocument();
});

test("tabs follow the arrow keys", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([]));
  renderDetail();
  const saves = await screen.findByRole("tab", { name: "Saves" });
  saves.focus();
  await userEvent.keyboard("{ArrowRight}");
  const shots = screen.getByRole("tab", { name: "Screenshots" });
  expect(shots).toHaveFocus();
  expect(shots).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByText("No screenshots yet")).toBeInTheDocument();
  await userEvent.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "DOSBox settings" })).toHaveFocus();
  await userEvent.keyboard("{ArrowRight}");
  expect(saves).toHaveFocus();
});

test("screenshots open in a viewer and delete after a second click", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(
    ok([shot("image0002.png", 1_700_000_100), shot("image0001.png", 1_700_000_000)]),
  );
  vi.mocked(commands.deleteScreenshot).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Screenshots" }));
  const thumbs = await screen.findAllByRole("button", { name: /^Screenshot from/ });
  expect(thumbs).toHaveLength(2);
  await userEvent.click(thumbs[1]);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText("2 of 2")).toBeInTheDocument();
  expect(within(dialog).getByRole("img")).toHaveAttribute(
    "src",
    "asset:///data/profiles/keen4-1/capture/image0001.png",
  );
  await userEvent.click(within(dialog).getByRole("button", { name: "Newer screenshot" }));
  expect(within(dialog).getByText("1 of 2")).toBeInTheDocument();

  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  expect(commands.deleteScreenshot).not.toHaveBeenCalled();
  await userEvent.click(within(dialog).getByRole("button", { name: "Really delete?" }));
  expect(commands.deleteScreenshot).toHaveBeenCalledWith("keen4-1", "image0002.png");
});

test("videos show their first frame and length, and open in a player", async () => {
  const video = (file: string, takenAt: number, thumbnail: string | null): Video => ({
    file,
    path: `/data/profiles/keen4-1/capture/${file}`,
    takenAt,
    size: 1000,
    duration: thumbnail ? 75 : null,
    thumbnail,
  });
  vi.mocked(commands.listVideos).mockResolvedValue(
    ok([
      video("video0002.avi", 1_700_000_100, null),
      video("video0001.avi", 1_700_000_000, "/thumbs/video0001.avi.png"),
    ]),
  );
  // Nothing recorded yet, so nothing to draw in jsdom.
  vi.mocked(commands.videoIndex).mockResolvedValue(
    ok({ width: 320, height: 200, frameUs: 14_286, frames: [], keyFrames: [], audio: null }),
  );
  vi.mocked(commands.deleteVideo).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Videos" }));
  const tiles = await screen.findAllByRole("button", { name: /^Video from/ });
  expect(tiles).toHaveLength(2);
  expect(tiles[1]).toHaveTextContent("1:15");
  expect(tiles[1].querySelector("img")).toHaveAttribute("src", "asset:///thumbs/video0001.avi.png");
  expect(tiles[0].querySelector("img")).toBeNull();

  await userEvent.click(tiles[1]);
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByRole("slider", { name: "Position" })).toBeInTheDocument();
  await vi.waitFor(() =>
    expect(commands.videoIndex).toHaveBeenCalledWith("keen4-1", "video0001.avi"),
  );
  expect(within(dialog).getByRole("button", { name: "Play" })).toBeDisabled();
  await userEvent.click(within(dialog).getByRole("button", { name: "Newer video" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Really delete?" }));
  expect(commands.deleteVideo).toHaveBeenCalledWith("keen4-1", "video0002.avi");
});

test("lists saves with their details and backups", async () => {
  renderDetail();
  expect(await screen.findByText("Slot 1 · pabba")).toBeInTheDocument();
  expect(
    screen.getByText("Score 48,100 · 13 lives · 45 shots · 2 levels done · Hard"),
  ).toBeInTheDocument();
  expect(screen.getByText("Slot 6")).toBeInTheDocument();
  expect(screen.getByText("Couldn’t read the details of this save.")).toBeInTheDocument();
  expect(await screen.findByText("Before playing")).toBeInTheDocument();
  expect(screen.getByText(/1 save \+ settings and high scores/)).toBeInTheDocument();
});

test("deleting a save asks first and can be undone", async () => {
  vi.mocked(commands.deleteSave).mockResolvedValue(ok(12));
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Delete Slot 1 · pabba" }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText(/backed up first/)).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  expect(commands.deleteSave).toHaveBeenCalledWith("keen4-1", "SAVEGAM0.CK4");
  expect(await screen.findByText(/^Save deleted\./)).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Undo" }));
  expect(commands.restoreBackup).toHaveBeenCalledWith(12, "SAVEGAM0.CK4", null, false);
  expect(await screen.findByText("Save brought back.")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument();
});

test("automatic backups sit in a closed group below the user's own", async () => {
  vi.mocked(commands.listBackups).mockResolvedValue(
    ok([{ ...backup, id: 8, reason: null, label: "Boss", auto: false }, backup]),
  );
  renderDetail();
  const group = (await screen.findByText("Automatic backups (1)")).closest("details")!;
  expect(group).not.toHaveAttribute("open");
  expect(within(group).getByText("Before playing")).toBeInTheDocument();
  expect(within(group).queryByText("Boss")).not.toBeInTheDocument();
  expect(screen.getByText("Boss")).toBeInTheDocument();
});

test("backs up everything with a label", async () => {
  vi.mocked(commands.createBackup).mockResolvedValue(ok({ ...backup, id: 8 }));
  renderDetail();
  await userEvent.type(await screen.findByLabelText("Label for the backup"), "Boss");
  await userEvent.click(screen.getByRole("button", { name: "Back up all" }));
  expect(commands.createBackup).toHaveBeenCalledWith("keen4-1", "Boss", null);
  expect(await screen.findByText("Backup made.")).toBeInTheDocument();
});

test("restores one save into another slot and warns about what it replaces", async () => {
  vi.mocked(commands.backupSaves).mockResolvedValue(ok([save("SAVEGAM0.CK4", 1, "old")]));
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Restore Before playing" }));
  const dialog = screen.getByRole("dialog");
  expect(await within(dialog).findByText("Slot 1 · old")).toBeInTheDocument();
  // Slot 1 holds "pabba" now.
  expect(within(dialog).getByText("Replaces Slot 1 · pabba; it’s backed up first.")).toBeVisible();
  expect(within(dialog).getByLabelText(/Also reset settings and high scores/)).not.toBeChecked();

  await userEvent.selectOptions(within(dialog).getByLabelText("Into"), "SAVEGAM2.CK4");
  expect(within(dialog).queryByText(/^Replaces/)).not.toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Restore this save" }));
  expect(commands.restoreBackup).toHaveBeenCalledWith(7, "SAVEGAM0.CK4", "SAVEGAM2.CK4", false);
  expect(await screen.findByText(/^Restored/)).toBeInTheDocument();
});

test("restores all saves, and settings and high scores only if asked", async () => {
  vi.mocked(commands.backupSaves).mockResolvedValue(ok([save("SAVEGAM0.CK4", 1, "old")]));
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Restore Before playing" }));
  const dialog = screen.getByRole("dialog");
  await userEvent.click(await within(dialog).findByRole("button", { name: "Restore all saves" }));
  expect(commands.restoreBackup).toHaveBeenLastCalledWith(7, null, null, false);

  await userEvent.click(await screen.findByRole("button", { name: "Restore Before playing" }));
  const again = screen.getByRole("dialog");
  await userEvent.click(within(again).getByLabelText(/Also reset settings and high scores/));
  await userEvent.click(within(again).getByRole("button", { name: "Restore everything" }));
  expect(commands.restoreBackup).toHaveBeenLastCalledWith(7, null, null, true);
});

test("restores an earlier version of a save", async () => {
  vi.mocked(commands.listSaves).mockResolvedValue(
    ok([{ ...save("SAVEGAM0.CK4", 1, "pabba"), versions: 2 }, save("SAVEGAM5.CK4", 6, null)]),
  );
  vi.mocked(commands.saveVersions).mockResolvedValue(
    ok([
      { backupId: 9, save: save("SAVEGAM0.CK4", 1, "before boss") },
      { backupId: 7, save: save("savegam0.ck4", 1, "start") },
    ]),
  );
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(null));
  renderDetail();
  // Only the save with earlier versions offers them.
  const open = await screen.findByRole("button", { name: "Earlier versions of Slot 1 · pabba" });
  expect(open).toHaveTextContent("2 earlier versions");
  expect(screen.getAllByRole("button", { name: /earlier versions/i })).toHaveLength(1);

  await userEvent.click(open);
  const dialog = screen.getByRole("dialog", { name: "Earlier versions of Slot 1 · pabba" });
  expect(await within(dialog).findByText("Slot 1 · before boss")).toBeInTheDocument();
  expect(commands.saveVersions).toHaveBeenCalledWith("keen4-1", "SAVEGAM0.CK4");
  await userEvent.click(within(dialog).getAllByRole("button", { name: /Restore the version/ })[1]);
  expect(commands.restoreBackup).toHaveBeenCalledWith(7, "savegam0.ck4", null, false);
  expect(await screen.findByText(/^Earlier version restored/)).toBeInTheDocument();
});

test("save actions are locked while the game runs", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...profile, running: true }]));
  renderDetail();
  expect(await screen.findByText(/The game is running/)).toBeInTheDocument();
  expect(await screen.findByRole("button", { name: "Delete Slot 1 · pabba" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Restore Before playing" })).toBeDisabled();
});

test("config tab saves the profile layer and shows warnings and the result", async () => {
  vi.mocked(commands.configView).mockResolvedValue(
    ok({
      global: "",
      globalWarnings: [],
      profile: {
        defaults: "[cpu]\ncpu_cycles = 20000\n",
        text: "# mine\n",
        warnings: [],
        effective: [{ section: "cpu", key: "cpu_cycles", value: "20000", layer: "defaults" }],
      },
    }),
  );
  vi.mocked(commands.saveConfig).mockResolvedValue(ok([{ line: 2, kind: "autoexec" }]));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "DOSBox settings" }));
  expect(await screen.findByText("[cpu] cpu_cycles")).toBeInTheDocument();
  expect(screen.getByText("BloogBase default")).toBeInTheDocument();

  const editor = screen.getByRole("textbox", { name: "Settings for this game" });
  await userEvent.clear(editor);
  await userEvent.type(editor, "[[autoexec]");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(commands.saveConfig).toHaveBeenCalledWith("keen4-1", "[autoexec]");
  expect(await screen.findByText(/Line 2: \[autoexec\] is ignored/)).toBeInTheDocument();
});

test("slot files follow the episode's numbering", () => {
  expect(slotFiles("galaxy", "SAVEGAM3.CK4").map((s) => [s.file, s.slot])[0]).toEqual([
    "SAVEGAM0.CK4",
    1,
  ]);
  expect(slotFiles("vorticons", "saved1.ck1")[8]).toEqual({ file: "saved9.ck1", slot: 9 });
  expect(slotFiles("dreams", "SAVEGM0.KDR")).toHaveLength(6);
});
