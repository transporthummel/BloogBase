import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  commands,
  type Backup,
  type Game,
  type IndexEntry,
  type IndexMod,
  type Install,
  type Mod,
  type Profile,
  type RuntimeStatus,
  type SaveDir,
  type SaveFile,
  type Screenshot,
  type Video,
  type ProfileWorlds,
} from "../bindings";
import { setControlsShown } from "../preferences";
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
    "folderSaves",
    "restoreFolder",
    "configView",
    "saveConfig",
    "resetConfig",
    "listMods",
    "modReadmes",
    "openModFolder",
    "modReport",
    "openGameFolder",
    "openModReadme",
    "gameReadmes",
    "openGameReadme",
    "deleteMod",
    "copyCaptures",
    "modInstalls",
    "setModInstall",
    "removeGame",
    "indexMods",
    "freeGames",
    "listScreenshots",
    "profileCover",
    "deleteScreenshot",
    "listVideos",
    "deleteVideo",
    "videoIndex",
    "openCaptureFolder",
    "oneButtonFiring",
    "profileFiring",
    "profileMachine",
    "profileCustomPatches",
    "setCustomPatches",
    "profileWorlds",
    "renameWorld",
    "setWorldFavourite",
    "deleteWorld",
    "activateWorld",
    "listVariants",
    "setVariant",
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
  dataFiles: [],
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
  missingVersions: [],
  standalone: false,
  sourceMod: false,
  saveDirs: [],
  playedOver: null,
  gameProfileId: null,
  worlds: false,
  activeWorld: null,
  modStatus: null,
  variantCount: 0,
  activeVariant: null,
  variantChosen: false,
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
  world: null,
  variant: null,
};

const ready: RuntimeStatus = {
  active: { id: "x", version: "0.83.0", path: "/x", managed: true },
  managed: null,
  pinnedVersion: "0.83.0",
  downloadAvailable: true,
  crtSwitchIgnored: false,
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
  vi.mocked(commands.gameReadmes).mockResolvedValue(ok([]));
  vi.mocked(commands.indexMods).mockResolvedValue(ok([]));
  vi.mocked(commands.freeGames).mockResolvedValue(ok([]));
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([]));
  vi.mocked(commands.listVideos).mockResolvedValue(ok([]));
  vi.mocked(commands.profileCover).mockResolvedValue(ok(null));
  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(false));
  vi.mocked(commands.profileFiring).mockResolvedValue(ok(null));
  vi.mocked(commands.profileMachine).mockResolvedValue(ok(null));
  vi.mocked(commands.profileCustomPatches).mockResolvedValue(ok(null));
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
  extends: null,
  keenwikiUrl: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss",
  homepage: "https://aurora.example/",
  pckf: "https://pckf.com/viewtopic.php?t=1",
  keenmodding: "https://keenmodding.org/viewtopic.php?t=2",
  status: null,
  addedAt: 1_700_000_000,
  profileId: "aurora-bliss",
};

/** Picks one of the actions in the hero's menu. */
async function pick(action: string) {
  await userEvent.click(await screen.findByRole("button", { name: "Options" }));
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
    ok([
      {
        ...profile,
        id: "keen4-1",
        modId: "aurora-bliss",
        label: "Aurora Bliss",
        playedOver: { profileId: "lego-keen", name: "Lego Keen" },
      },
    ]),
  );
  const pack: Mod = {
    ...aurora,
    id: "pack",
    name: "Pack",
    extends: "aurora-bliss",
    profileId: "pack",
  };
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora, pack]));
  renderDetail();
  expect(await screen.findByRole("heading", { name: "Aurora Bliss" })).toBeInTheDocument();
  const author = await screen.findByRole("button", { name: "Dave216" });
  expect(author.parentElement).toHaveTextContent(/^by Dave216, Ceilick · Released on May 1, 2012$/);
  // Without a page, a name is plain text.
  expect(screen.queryByRole("button", { name: "Ceilick" })).toBeNull();
  await userEvent.click(author);
  expect(openUrl).toHaveBeenCalledWith("https://keenwiki.shikadi.net/wiki/User:Dave216");
  expect(screen.getByText("A colourful journey.")).toBeInTheDocument();
  // The mods played over it.
  expect(screen.getByText("Played over it:").parentElement).toHaveTextContent(
    /^Played over it: Pack$/,
  );
  expect(screen.getByText("Lego Keen").parentElement).toHaveTextContent(
    `Runs on ${keen4.shortTitle} Unknown version and Lego Keen`,
  );
  await userEvent.click(screen.getByRole("button", { name: "KeenWiki" }));
  expect(openUrl).toHaveBeenCalledWith(aurora.keenwikiUrl);
  await userEvent.click(screen.getByRole("button", { name: "PCKF" }));
  expect(openUrl).toHaveBeenCalledWith(aurora.pckf);
  await userEvent.click(screen.getByRole("button", { name: "Keen: Modding" }));
  expect(openUrl).toHaveBeenCalledWith(aurora.keenmodding);
  await userEvent.click(screen.getByRole("button", { name: "Website" }));
  expect(openUrl).toHaveBeenCalledWith(aurora.homepage);
  // Not in the index yet.
  expect(screen.getByRole("button", { name: "Suggest it for the mod list" })).toBeInTheDocument();
  expect(screen.getByText("Sessions").nextSibling).toHaveTextContent("1");
  vi.mocked(commands.openModFolder).mockResolvedValue(ok(null));
  await pick("Open mod folder");
  expect(commands.openModFolder).toHaveBeenCalledWith("aurora-bliss");
  // The game's folder is on the game's own page.
  await userEvent.click(screen.getByRole("button", { name: "Options" }));
  expect(screen.queryByRole("menuitem", { name: "Open game folder" })).not.toBeInTheDocument();
});

/** An index entry of `modId` with `status`. */
const indexed = (modId: string, status: IndexEntry["status"]): IndexMod => ({
  entry: {
    id: modId,
    name: modId,
    game: "keen4",
    requires: [],
    authors: [],
    added: "2020-01-01",
    type: "episode",
    status,
    links: {},
    downloads: [],
    recipe: { type: "episode" },
  },
  modId,
  updateAvailable: false,
  new: false,
});

test("a mod's page says it was abandoned", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([{ ...aurora, indexId: "aurora-bliss" }]));
  vi.mocked(commands.indexMods).mockResolvedValue(ok([indexed("aurora-bliss", "abandoned")]));
  renderDetail();
  expect(await screen.findByText("Abandoned")).toHaveClass("text-warning");
});

test("a KeenWiki mod's page says what its page said", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([{ ...aurora, status: "alpha" }]));
  renderDetail();
  expect(await screen.findByText("Alpha")).toBeInTheDocument();
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
  const mine = { ...profile, modId: "aurora-bliss", label: "Aurora Bliss" };
  const others = [{ ...profile, id: "keen5-1", gameId: "keen5" }];
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([mine, ...others]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.deleteMod).mockImplementation(async () => {
    vi.mocked(commands.listProfiles).mockResolvedValue(ok(others));
    return { status: "ok", data: null };
  });
  const client = new QueryClient();
  client.setQueryData(["cover", "keen4-1", null], { src: "old.png", scrambled: false });
  client.setQueryData(["cover", "keen5-1", profile.installId], {
    src: "keen5.png",
    scrambled: false,
  });
  vi.mocked(commands.listVideos).mockResolvedValue(
    ok([{ ...shot("video0001.avi", 1), size: 1000, duration: null, thumbnail: null }]),
  );
  renderDetail(client);
  await pick("Remove");
  const dialog = screen.getByRole("dialog");
  expect(await within(dialog).findByText(/This also deletes 1 video\./)).toBeInTheDocument();
  // They can be copied first.
  vi.mocked(openDialog).mockResolvedValue("C:/Keepers");
  vi.mocked(commands.copyCaptures).mockResolvedValue(
    ok({ files: 1, folders: ["C:/Keepers/Aurora Bliss"] }),
  );
  await userEvent.click(within(dialog).getByRole("button", { name: "Copy to a folder…" }));
  expect(commands.copyCaptures).toHaveBeenCalledWith(["keen4-1"], null, "C:/Keepers");
  // One of each: said in the singular.
  expect(dialog).toHaveTextContent("This also deletes 1 video. Unlike saves, no backup keeps it.");
  expect(await within(dialog).findByText(/^Copied 1 file to:/)).toHaveTextContent(
    "C:/Keepers/Aurora Bliss",
  );
  await userEvent.click(within(dialog).getByRole("checkbox"));
  await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(commands.deleteMod).toHaveBeenCalledWith("aurora-bliss", false);
  expect(navigate).toHaveBeenCalledWith({ to: "/" });
  // Installed again, it and the mods played over it, removed with it, get
  // the same ids and must not show the old covers; the others stay.
  await vi.waitFor(() => expect(client.getQueryData(["cover", "keen4-1", null])).toBeUndefined());
  expect(client.getQueryData(["cover", "keen5-1", profile.installId])).toBeDefined();
});

test("without saves, removing asks only about backups that would go", async () => {
  const mine = { ...profile, modId: "aurora-bliss", label: "Aurora Bliss", saveCount: 0 };
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([mine]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.deleteMod).mockResolvedValue(ok(null));
  // An automatic backup goes either way: nothing to ask.
  renderDetail();
  await pick("Remove");
  await vi.waitFor(() => expect(commands.listBackups).toHaveBeenCalled());
  expect(within(screen.getByRole("dialog")).queryByRole("checkbox")).toBeNull();
  // No screenshots or videos, nothing said about them.
  expect(within(screen.getByRole("dialog")).queryByText(/also deletes/)).toBeNull();

  cleanup();
  const manual = { ...backup, id: 8, label: "mine", reason: null, auto: false };
  vi.mocked(commands.listBackups).mockResolvedValue(ok([backup, manual]));
  renderDetail();
  await pick("Remove");
  const dialog = screen.getByRole("dialog");
  await userEvent.click(await within(dialog).findByRole("checkbox", { name: "Keep the backup" }));
  await userEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(commands.deleteMod).toHaveBeenCalledWith("aurora-bliss", false);
});

test("a mod just installed isn't called missing while the list is fetched again", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  // The library's list from before the install.
  client.setQueryData(["profiles"], []);
  let resolve!: (v: ReturnType<typeof ok<Profile[]>>) => void;
  vi.mocked(commands.listProfiles).mockReturnValue(new Promise((r) => (resolve = r)));
  void client.invalidateQueries({ queryKey: ["profiles"] });
  renderDetail(client);
  expect(screen.queryByText("This game isn’t in the library any more.")).toBeNull();
  resolve(ok([profile]));
  expect(await screen.findByRole("heading", { name: keen4.title })).toBeInTheDocument();
  expect(screen.queryByText("This game isn’t in the library any more.")).toBeNull();
});

test("a profile that is gone says so", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([]));
  renderDetail();
  expect(await screen.findByText("This game isn’t in the library any more.")).toBeInTheDocument();
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
  await userEvent.click(await screen.findByRole("button", { name: "Options" }));
  expect(screen.getByRole("menuitem", { name: "Replace with a local copy…" })).toBeDisabled();
});

test("a mod from the index can't be replaced with a local copy", async () => {
  const mine = { ...profile, modId: "aurora-bliss", label: "Aurora Bliss" };
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([mine]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([{ ...aurora, indexId: "aurora-bliss" }]));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Options" }));
  expect(screen.getByRole("menuitem", { name: "Open mod folder" })).toBeInTheDocument();
  expect(screen.queryByRole("menuitem", { name: "Replace with a local copy…" })).toBeNull();
});

test("a mod's More menu reports that it won't start, once it has been started", async () => {
  const mine = { ...profile, modId: "aurora-bliss", label: "Aurora Bliss" };
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([mine]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReport).mockResolvedValue(ok("### Mod doesn't start: Aurora Bliss"));
  renderDetail();
  await pick("Mod won’t start?");
  expect(
    await screen.findByRole("dialog", { name: "Report a problem with Aurora Bliss" }),
  ).toBeInTheDocument();
  expect(commands.modReport).toHaveBeenCalledWith("keen4-1");

  cleanup();
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...mine, sessionCount: 0 }]));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Options" }));
  expect(screen.queryByRole("menuitem", { name: "Mod won’t start?" })).not.toBeInTheDocument();
});

test("a game's page credits id Software, with the version's release date and text", async () => {
  const version = { id: "keen4-1.4-ega", label: "v1.4 EGA", files: {}, shareware: true };
  const demo = { ...version, id: "keen4-demo", label: "Special demo", kind: "demo" as const };
  vi.mocked(commands.listGames).mockResolvedValue([
    {
      ...keen4,
      versions: [
        { ...version, released: "1992-02-27" },
        { ...demo, released: "1991-10" },
      ],
    },
  ]);
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, versionId: "keen4-1.4-ega" }]),
  );
  const { unmount } = renderDetail();
  const developer = await screen.findByRole("button", { name: "id Software" });
  expect(developer.parentElement).toHaveTextContent(/^by id Software · Released on Feb 27, 1992$/);
  await userEvent.click(developer);
  expect(openUrl).toHaveBeenCalledWith("https://www.idsoftware.com");
  expect(screen.getByText("Released on Feb 27, 1992")).toBeInTheDocument();
  expect(screen.getByText(/the Shikadi plan to destroy the galaxy/)).toBeInTheDocument();
  unmount();

  // A demo has its own text.
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...profile, versionId: "keen4-demo" }]));
  renderDetail();
  expect(await screen.findByText("Released in October 1991")).toBeInTheDocument();
  expect(screen.getByText(/A demo of Goodbye, Galaxy!/)).toBeInTheDocument();
  expect(screen.queryByText(/the Shikadi plan to destroy the galaxy/)).toBeNull();
});

test("a game's page opens the folder with its files", async () => {
  vi.mocked(commands.openGameFolder).mockResolvedValue(ok(null));
  renderDetail();
  await pick("Open game folder");
  expect(commands.openGameFolder).toHaveBeenCalledWith("keen4-1");
  // No mod folder for a game.
  await userEvent.click(screen.getByRole("button", { name: "Options" }));
  expect(screen.queryByRole("menuitem", { name: "Open mod folder" })).not.toBeInTheDocument();
});

test("a game is removed from its page, with its mods if asked", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([
      profile,
      { ...profile, id: "aurora-bliss", modId: "aurora-bliss", label: "Aurora Bliss" },
      { ...profile, id: "other", installId: "keen4-2", modId: "other", label: "Other" },
    ]),
  );
  vi.mocked(commands.removeGame).mockImplementation(async () => {
    vi.mocked(commands.listProfiles).mockResolvedValue(
      ok([{ ...profile, id: "other", installId: "keen4-2", modId: "other", label: "Other" }]),
    );
    return { status: "ok", data: null };
  });
  const client = new QueryClient();
  for (const [id, installId] of [
    ["keen4-1", "keen4-1"],
    ["aurora-bliss", "keen4-1"],
    ["other", "keen4-2"],
  ]) {
    client.setQueryData(["cover", id, installId], { src: `${id}.png`, scrambled: false });
  }
  vi.mocked(commands.listScreenshots).mockImplementation(async (id) =>
    ok(id === "keen4-1" ? [shot("image0001.png", 1), shot("image0002.png", 2)] : []),
  );
  vi.mocked(commands.listVideos).mockImplementation(async (id) =>
    ok(
      id === "aurora-bliss"
        ? [{ ...shot("video0001.avi", 1), size: 1000, duration: null, thumbnail: null }]
        : [],
    ),
  );
  renderDetail(client);
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Options" }));
  // The game's own entry, not also the one for a mod.
  expect(screen.getAllByRole("menuitem", { name: "Remove" })).toHaveLength(1);
  await user.click(screen.getByRole("menuitem", { name: "Remove" }));
  const dialog = screen.getByRole("dialog");
  // Only the mod on this copy of the game counts.
  const withMods = within(dialog).getByRole("checkbox", {
    name: /^Also remove .+, the mod played on it$/,
  });
  expect(dialog).toHaveTextContent("Otherwise it moves to another copy");
  // Screenshots and videos go without a backup: the game's, and with its
  // mods theirs.
  expect(await within(dialog).findByText(/This also deletes 2 screenshots\./)).toBeInTheDocument();
  await user.click(withMods);
  expect(
    await within(dialog).findByText(/This also deletes 2 screenshots and 1 video\./),
  ).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "Remove" }));
  expect(commands.removeGame).toHaveBeenCalledWith("keen4-1", true, true);
  expect(navigate).toHaveBeenCalledWith({ to: "/" });
  // The game and its mod are gone; the other copy's mod keeps its cover.
  await vi.waitFor(() =>
    expect(client.getQueryData(["cover", "keen4-1", "keen4-1"])).toBeUndefined(),
  );
  expect(client.getQueryData(["cover", "aurora-bliss", "keen4-1"])).toBeUndefined();
  expect(client.getQueryData(["cover", "other", "keen4-2"])).toBeDefined();
});

test("without DOSBox, Play is red and says why", async () => {
  vi.mocked(commands.runtimeStatus).mockResolvedValue(ok({ ...ready, active: null }));
  renderDetail();
  const play = await screen.findByRole("button", { name: "DOSBox missing" });
  expect(play).toBeDisabled();
  expect(play.className).toContain("text-danger");
  expect(screen.getByText("BloogBase needs DOSBox Staging to start games.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Download DOSBox Staging 0.83.0" })).toBeEnabled();
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
        missingVersions: [],
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
  await userEvent.click(screen.getByRole("button", { name: "Options" }));
  // The mod's own entry, not also the one for a game.
  expect(screen.getAllByRole("menuitem", { name: "Remove" })).toHaveLength(1);

  // Removing it doesn't mention a game that stays.
  await userEvent.click(screen.getByRole("menuitem", { name: "Remove" }));
  const dialog = screen.getByRole("dialog");
  expect(dialog).toHaveTextContent("The mod’s files are deleted.");
  expect(dialog).not.toHaveTextContent("The game itself stays");
});

const waiting: Profile = {
  ...profile,
  installId: null,
  versionId: null,
  modId: "aurora-bliss",
  label: "Aurora Bliss",
  missingGame: "Commander Keen 4 (v1.4 EGA)",
  missingVersions: [],
};

test("a mod waiting for a shareware episode leads to the free download", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([waiting]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.freeGames).mockResolvedValue(
    ok([{ gameId: "keen4", versionId: "keen4-v14e-sw", setup: true, size: 1, installed: false }]),
  );
  renderDetail();
  expect(await screen.findByText("Download for free")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Add game" })).toBeNull();
});

test("a mod waiting for any other game opens the add dialog", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([waiting]));
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Add game" }));
  expect(screen.getByRole("dialog", { name: "Add a game or mod" })).toBeInTheDocument();
  expect(screen.queryByText("Download for free")).toBeNull();
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
        pdf: false,
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
      { file: "changes.txt", text: "v1.1: fixed a crash", html: false, pdf: false, word: false },
      {
        file: "Hints Sheet.txt",
        text: "The key is behind the wall",
        html: false,
        pdf: false,
        word: false,
      },
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
    ok([{ file: "Files/README.HTM", text: "", html: true, pdf: false, word: false }]),
  );
  vi.mocked(commands.openModReadme).mockResolvedValue(ok(null));
  renderDetail();
  const panel = await screen.findByRole("tabpanel");
  expect(panel).toHaveTextContent("Files/README.HTM");
  expect(panel.querySelector("pre")).toBeNull();
  await userEvent.click(within(panel).getByRole("button", { name: "Open in browser" }));
  expect(commands.openModReadme).toHaveBeenCalledWith("aurora-bliss", "Files/README.HTM");
});

test("a PDF document of a mod opens in its app", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([{ file: "Manual.pdf", text: "", html: false, pdf: true, word: false }]),
  );
  vi.mocked(commands.openModReadme).mockResolvedValue(ok(null));
  renderDetail();
  const panel = await screen.findByRole("tabpanel");
  expect(panel).toHaveTextContent("PDF document");
  expect(panel.querySelector("pre")).toBeNull();
  await userEvent.click(within(panel).getByRole("button", { name: "Open in default app" }));
  expect(commands.openModReadme).toHaveBeenCalledWith("aurora-bliss", "Manual.pdf");
});

test("a Word document of a mod shows its text and opens in its app", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([
      { file: "keenstatic.doc", text: "World of Static", html: false, pdf: false, word: true },
      { file: "hints.doc", text: "", html: false, pdf: false, word: true },
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
  expect(await screen.findByRole("tab", { name: /^Saves/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(screen.queryByRole("tab", { name: "Readme" })).toBeNull();
});

test("a game without documents has no README tab", async () => {
  renderDetail();
  expect(await screen.findByRole("tab", { name: /^Saves/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  expect(commands.gameReadmes).toHaveBeenCalledWith("keen4-1");
  expect(commands.modReadmes).not.toHaveBeenCalled();
  expect(screen.queryByRole("tab", { name: "Readme" })).toBeNull();
});

test("a game's own documents are its README tab, web pages opening outside", async () => {
  vi.mocked(commands.gameReadmes).mockResolvedValue(
    ok([
      { file: "README.DOC", text: "Commander Keen 4", html: false, pdf: false, word: false },
      { file: "ORDER.HTM", text: "", html: true, pdf: false, word: false },
    ]),
  );
  vi.mocked(commands.openGameReadme).mockResolvedValue(ok(null));
  renderDetail();
  const tab = await screen.findByRole("tab", { name: "Readme" });
  expect(tab).toHaveAttribute("aria-selected", "true");
  expect(screen.getByText(/sets up this game for you/)).toBeInTheDocument();
  const panel = screen.getByRole("tabpanel");
  expect(panel).toHaveTextContent("Commander Keen 4");
  await userEvent.selectOptions(
    within(panel).getByRole("combobox", { name: "File:" }),
    "ORDER.HTM",
  );
  await userEvent.click(within(panel).getByRole("button", { name: "Open in browser" }));
  expect(commands.openGameReadme).toHaveBeenCalledWith("keen4-1", "ORDER.HTM");
  expect(commands.modReadmes).not.toHaveBeenCalled();
});

test("a mod's README tab never shows its game's documents", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.listMods).mockResolvedValue(ok([aurora]));
  renderDetail();
  expect(await screen.findByRole("tab", { name: /^Saves/ })).toBeInTheDocument();
  expect(commands.gameReadmes).not.toHaveBeenCalled();
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

test("says which machine DOSBox runs as for a mod that needs another", async () => {
  renderDetail();
  await screen.findByRole("heading", { name: keen4.title });
  expect(commands.profileMachine).toHaveBeenCalledWith("keen4-1");
  expect(screen.queryByText(/DOSBox runs as/)).toBeNull();
  cleanup();

  vi.mocked(commands.profileMachine).mockResolvedValue(ok("paradiseSvga"));
  renderDetail();
  expect(await screen.findByText(/DOSBox runs as a Paradise SVGA card/)).toBeInTheDocument();
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

test("no placeholder flashes up while the title screen is read", async () => {
  let resolve!: (v: ReturnType<typeof ok<{ path: string; scrambled: boolean }>>) => void;
  vi.mocked(commands.profileCover).mockReturnValue(new Promise((r) => (resolve = r)));
  const { container } = renderDetail();
  await screen.findByRole("heading", { name: keen4.title });
  // The placeholder shows the episode number.
  expect(container.querySelector(".font-pixel.text-7xl")).toBeNull();
  resolve(ok({ path: "/data/cache/covers/keen4-1.png", scrambled: false }));
  await vi.waitFor(() =>
    expect(container.querySelector("img")?.getAttribute("src")).toMatch(/keen4-1\.png/),
  );
  expect(container.querySelector(".font-pixel.text-7xl")).toBeNull();
});

test("tabs follow the arrow keys", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([]));
  renderDetail();
  // A game without a README opens on its first tab, the saves, its keys next.
  const saves = await screen.findByRole("tab", { name: /^Saves/ });
  expect(screen.getAllByRole("tab").map((x) => x.textContent)).toEqual([
    "Saves 2",
    "Controls",
    "Screenshots",
    "Videos",
    "Settings",
  ]);
  expect(saves).toHaveAttribute("aria-selected", "true");
  saves.focus();
  await userEvent.keyboard("{ArrowRight}{ArrowRight}");
  const shots = screen.getByRole("tab", { name: /^Screenshots/ });
  expect(shots).toHaveFocus();
  expect(shots).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByText("No screenshots yet")).toBeInTheDocument();
  await userEvent.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "Settings" })).toHaveFocus();
  await userEvent.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: /^Saves/ })).toHaveFocus();
});

test("screenshots open in a viewer and delete after a second click", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(
    ok([shot("image0002.png", 1_700_000_100), shot("image0001.png", 1_700_000_000)]),
  );
  vi.mocked(commands.deleteScreenshot).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /^Screenshots/ }));
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

test("a tab copies its screenshots to a folder", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(ok([shot("image0001.png", 1)]));
  vi.mocked(openDialog).mockResolvedValue("C:/Keepers");
  vi.mocked(commands.copyCaptures).mockResolvedValue(
    ok({ files: 1, folders: ["C:/Keepers/Keen 4"] }),
  );
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /^Screenshots/ }));
  await userEvent.click(await screen.findByRole("button", { name: "Copy to a folder…" }));
  // Only screenshots, not the videos too.
  expect(commands.copyCaptures).toHaveBeenCalledWith(["keen4-1"], "screenshot", "C:/Keepers");
  expect(await screen.findByText(/^Copied 1 file to:/)).toHaveTextContent("C:/Keepers/Keen 4");
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
  await userEvent.click(await screen.findByRole("tab", { name: /^Videos/ }));
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

test("saves sharing a slot show their file names", async () => {
  // As Foray in the Forest: its two builds save as .CKS and .CKC.
  vi.mocked(commands.listSaves).mockResolvedValue(
    ok([
      save("SAVEGAM0.CKC", 1, null),
      save("SAVEGAM0.CKS", 1, "pabba"),
      save("SAVEGAM1.CKS", 2, null),
    ]),
  );
  renderDetail();
  expect(await screen.findByText("Slot 1 (SAVEGAM0.CKS) · pabba")).toBeInTheDocument();
  expect(screen.getByText("Slot 1 (SAVEGAM0.CKC)")).toBeInTheDocument();
  expect(screen.getByText("Slot 2")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Delete Slot 1 (SAVEGAM0.CKC)" })).toBeInTheDocument();
});

test("a launcher's saves are grouped under the names of its folders", async () => {
  // As Revival of Bliss: Keen, Craig and the epilogue each save in a folder.
  const saveDirs: SaveDir[] = [
    { dir: "KEEN", name: "Keen" },
    { dir: "CRAIG", name: "Craig", oldRoot: true },
    { dir: "FUTURE", name: "Epilogue", translations: { de: { name: "Epilog" } } },
  ];
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...profile, saveDirs }]));
  vi.mocked(commands.listSaves).mockResolvedValue(
    ok([
      save("KEEN/SAVEGAM0.CK4", 1, "pabba"),
      save("CRAIG/SAVEGAM0.CK4", 1, null),
      save("CRAIG/SAVEGAM4.CK4", 5, "boss"),
    ]),
  );
  vi.mocked(commands.backupSaves).mockResolvedValue(ok([save("CRAIG/SAVEGAM0.CK4", 1, "old")]));
  renderDetail();
  const keen = (await screen.findByRole("heading", { name: "Keen" })).closest("section")!;
  expect(within(keen).getByText("Slot 1 · pabba")).toBeInTheDocument();
  const craig = screen.getByRole("heading", { name: "Craig" }).closest("section")!;
  // Each folder numbers its own slots.
  expect(within(craig).getByText("Slot 1")).toBeInTheDocument();
  expect(within(craig).getByText("Slot 5 · boss")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Epilogue" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Delete Craig · Slot 1" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Restore Before playing" }));
  const dialog = screen.getByRole("dialog");
  expect(await within(dialog).findByText("Craig · Slot 1 · old")).toBeInTheDocument();
  expect(within(dialog).getByText("Replaces Craig · Slot 1; it’s backed up first.")).toBeVisible();
  await userEvent.selectOptions(within(dialog).getByLabelText("Into"), "CRAIG/SAVEGAM2.CK4");
  expect(within(dialog).queryByText(/^Replaces/)).not.toBeInTheDocument();
});

test("deleting a save asks first and can be undone", async () => {
  vi.mocked(commands.deleteSave).mockImplementation(async () => {
    vi.mocked(commands.listBackups).mockResolvedValue(
      ok([{ ...backup, id: 12, reason: "beforeDelete" }, backup]),
    );
    return ok(12);
  });
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(true));
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

test("deleting a save can't be undone once its backup is deleted too", async () => {
  vi.mocked(commands.deleteSave).mockResolvedValue(ok(7));
  vi.mocked(commands.deleteBackup).mockImplementation(async () => {
    vi.mocked(commands.listBackups).mockResolvedValue(ok([]));
    return ok(null);
  });
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Delete Slot 1 · pabba" }));
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }));
  expect(await screen.findByRole("button", { name: "Undo" })).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Delete backup Before playing" }));
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Delete" }));
  await vi.waitFor(() =>
    expect(screen.queryByRole("button", { name: "Undo" })).not.toBeInTheDocument(),
  );
  expect(screen.getByText("Save deleted.")).toBeInTheDocument();
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
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(true));
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
  expect(
    await screen.findByText("Restored. What was there before was backed up."),
  ).toBeInTheDocument();
});

test("restores all saves, and settings and high scores only if asked", async () => {
  vi.mocked(commands.backupSaves).mockResolvedValue(ok([save("SAVEGAM0.CK4", 1, "old")]));
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(true));
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
  vi.mocked(commands.restoreBackup).mockResolvedValue(ok(true));
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

test("imports the saves of an old copy straight from a folder, not as a backup", async () => {
  const folder = String.raw`C:\Games\KEEN4`;
  vi.mocked(openDialog).mockResolvedValue(folder);
  vi.mocked(commands.folderSaves).mockResolvedValue(
    ok({ saves: [save("SAVEGAM3.CK4", 4, "old")], settings: 1, loose: false }),
  );
  // Only settings to replace: nothing was backed up.
  vi.mocked(commands.restoreFolder).mockResolvedValue(ok(false));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Import from folder…" }));
  expect(openDialog).toHaveBeenCalledWith({ directory: true });
  expect(commands.folderSaves).toHaveBeenCalledWith("keen4-1", folder);
  const dialog = await screen.findByRole("dialog", { name: "Import saves from “KEEN4”" });
  expect(within(dialog).getByText("Slot 4 · old")).toBeInTheDocument();
  expect(within(dialog).queryByLabelText("Part")).not.toBeInTheDocument();
  expect(within(dialog).getByLabelText(/Also reset settings and high scores/)).not.toBeChecked();
  await userEvent.click(within(dialog).getByRole("button", { name: "Restore all saves" }));
  expect(commands.restoreFolder).toHaveBeenCalledWith("keen4-1", folder, null, null, null, false);
  expect(await screen.findByText("Saves imported.")).toBeInTheDocument();
  expect(commands.importBackup).not.toHaveBeenCalled();
});

test("saves from a folder that names no part of a launcher go into the part chosen", async () => {
  const saveDirs: SaveDir[] = [
    { dir: "KEEN", name: "Keen" },
    { dir: "CRAIG", name: "Craig", oldRoot: true },
  ];
  vi.mocked(commands.listProfiles).mockResolvedValue(ok([{ ...profile, saveDirs }]));
  vi.mocked(commands.listSaves).mockResolvedValue(ok([save("CRAIG/SAVEGAM0.CK4", 1, "mine")]));
  const folder = String.raw`C:\Downloads\saves`;
  vi.mocked(openDialog).mockResolvedValue(folder);
  vi.mocked(commands.folderSaves).mockResolvedValue(
    ok({ saves: [save("SAVEGAM0.CK4", 1, "forum")], settings: 0, loose: true }),
  );
  vi.mocked(commands.restoreFolder).mockResolvedValue(ok(true));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Import from folder…" }));
  const dialog = await screen.findByRole("dialog", { name: "Import saves from “saves”" });
  // Nothing goes anywhere before the part is chosen.
  expect(within(dialog).getByRole("button", { name: "Restore all saves" })).toBeDisabled();
  expect(within(dialog).getByRole("button", { name: "Restore this save" })).toBeDisabled();
  await userEvent.selectOptions(within(dialog).getByLabelText("Part"), "CRAIG");
  expect(within(dialog).getByText("Craig · Slot 1 · forum")).toBeInTheDocument();
  expect(
    within(dialog).getByText("Replaces Craig · Slot 1 · mine; it’s backed up first."),
  ).toBeVisible();
  await userEvent.click(within(dialog).getByRole("button", { name: "Restore all saves" }));
  expect(commands.restoreFolder).toHaveBeenCalledWith(
    "keen4-1",
    folder,
    "CRAIG",
    null,
    null,
    false,
  );
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
  await userEvent.click(await screen.findByRole("tab", { name: "Settings" }));
  // The result is folded away.
  expect(screen.queryByText("[cpu] cpu_cycles")).toBeNull();
  await userEvent.click(await screen.findByRole("button", { name: "What DOSBox gets" }));
  expect(await screen.findByText("[cpu] cpu_cycles")).toBeInTheDocument();
  expect(screen.getByText("BloogBase default")).toBeInTheDocument();

  const editor = await screen.findByRole("textbox", { name: "DOSBox settings for this game" });
  await userEvent.clear(editor);
  await userEvent.type(editor, "[[autoexec]");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(commands.saveConfig).toHaveBeenCalledWith("keen4-1", "[autoexec]");
  expect(await screen.findByText(/Line 2: \[autoexec\] is ignored/)).toBeInTheDocument();
});

test("a game's settings tab saves its custom patches and names a reserved line", async () => {
  vi.mocked(commands.configView).mockResolvedValue(
    ok({ global: "", globalWarnings: [], profile: null }),
  );
  vi.mocked(commands.profileCustomPatches).mockResolvedValue(
    ok({ kind: "game", text: "", status: null }),
  );
  vi.mocked(commands.setCustomPatches).mockResolvedValueOnce(
    ok({ line: 2, problem: "reserved", word: "%end" }),
  );
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Settings" }));
  // Folded away while there are none.
  const fold = await screen.findByRole("button", { name: "Custom patches" });
  expect(fold).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(fold);
  const field = await screen.findByRole("textbox", { name: "Custom patches" });
  await userEvent.type(field, "%patch $10 $01\n%end");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(commands.setCustomPatches).toHaveBeenCalledWith("keen4-1", "%patch $10 $01\n%end");
  expect(
    await screen.findByText("Line 2: BloogBase writes %end itself. Please leave the line out."),
  ).toBeInTheDocument();

  vi.mocked(commands.setCustomPatches).mockResolvedValueOnce(ok(null));
  await userEvent.type(field, "{Backspace}{Backspace}{Backspace}{Backspace}");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(commands.setCustomPatches).toHaveBeenLastCalledWith("keen4-1", "%patch $10 $01\n");
  await vi.waitFor(() => expect(commands.profileCustomPatches).toHaveBeenCalledTimes(2));
});

test("a mod's settings tab says whether it gets the game's custom patches", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "aurora-bliss", label: "Aurora Bliss" }]),
  );
  vi.mocked(commands.configView).mockResolvedValue(
    ok({ global: "", globalWarnings: [], profile: null }),
  );
  vi.mocked(commands.profileCustomPatches).mockResolvedValue(
    ok({
      kind: "mod",
      status: "ownProgram",
      gameProfileId: "keen4-1",
      gameLabel: "Keen 4 v1.4",
    }),
  );
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Settings" }));
  expect(
    await screen.findByText(/doesn't get the custom patches of Keen 4 v1\.4: it starts a program/),
  ).toBeInTheDocument();
  expect(screen.getByText("Edit them on the game copy")).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: "Custom patches" })).toBeNull();
});

test("slot files follow the episode's numbering", () => {
  expect(slotFiles("galaxy", "SAVEGAM3.CK4").map((s) => [s.file, s.slot])[0]).toEqual([
    "SAVEGAM0.CK4",
    1,
  ]);
  expect(slotFiles("vorticons", "saved1.ck1")[8]).toEqual({ file: "saved9.ck1", slot: 9 });
  expect(slotFiles("dreams", "SAVEGM0.KDR")).toHaveLength(6);
});

test("the tabs count saves, screenshots and videos, leaving out none", async () => {
  vi.mocked(commands.listScreenshots).mockResolvedValue(
    ok([shot("image0001.png", 1_700_000_000), shot("image0002.png", 1_700_000_100)]),
  );
  renderDetail();
  expect(await screen.findByRole("tab", { name: "Saves 2" })).toBeInTheDocument();
  expect(await screen.findByRole("tab", { name: "Screenshots 2" })).toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Videos" })).toBeInTheDocument();
});

const randomiser: ProfileWorlds = {
  definition: {
    new: { type: "batch", file: "RNDKEEN1.BAT" },
    play: { type: "patch", patcher: "CK1PATCH.EXE", patch: "RNDKEEN1.PAT", args: [] },
    files: ["RNDLV*.CK1", "RNDKEEN1.PAT"],
    seed: { arg: "/SEED", max: 65535, read: null },
    options: [
      {
        id: "startpogo",
        arg: "/STARTPOGO",
        type: "flag",
        min: null,
        max: null,
        default: null,
        label: { en: "Start with the pogo stick", de: "Mit Pogo-Stick starten" },
      },
      {
        id: "startammo",
        arg: "/STARTAMMO",
        type: "number",
        min: 0,
        max: 99,
        default: 0,
        label: { en: "Starting ammo", de: "Munition zu Beginn" },
      },
    ],
  },
  worlds: [
    {
      id: 2,
      name: null,
      seed: 4711,
      options: { startpogo: 1, startammo: 15 },
      createdAt: 1_700_000_000,
      lastPlayedAt: 1_700_100_000,
      favourite: false,
      active: true,
      saves: 2,
    },
    {
      id: 1,
      name: "Hard one",
      seed: 12,
      options: { startpogo: 0, startammo: 0 },
      createdAt: 1_690_000_000,
      lastPlayedAt: 1_690_000_000,
      favourite: false,
      active: false,
      saves: 0,
    },
  ],
  active: 2,
  lastOptions: { startpogo: 1, startammo: 15 },
};

function withWorlds(activeWorld: number | null = 2) {
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...profile, modId: "rnd", label: "Keen 1 Randomiser", worlds: true, activeWorld }]),
  );
  vi.mocked(commands.profileWorlds).mockResolvedValue(ok(randomiser));
  vi.mocked(commands.launchProfile).mockResolvedValue(ok(null));
  // Straight to the game, without the controls first.
  setControlsShown("galaxy");
}

test("a mod with worlds opens on them, the first tab", async () => {
  withWorlds();
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([{ file: "README.TXT", text: "Hello.", html: false, pdf: false, word: false }]),
  );
  renderDetail();
  const tab = await screen.findByRole("tab", { name: "Worlds 2" });
  expect(tab).toHaveAttribute("aria-selected", "true");
  expect(screen.getAllByRole("tab")[0]).toBe(tab);
  expect(screen.getAllByRole("tab")[1]).toHaveTextContent("Readme");
});

test("a mod with worlds continues the one played last, or starts a new one with a seed", async () => {
  withWorlds();
  renderDetail();
  expect(await screen.findByText("Random worlds")).toBeInTheDocument();
  await userEvent.click(await screen.findByRole("button", { name: "Continue world" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      { type: "continue", id: 2 },
      null,
      "en",
    ),
  );

  await userEvent.click(screen.getByRole("button", { name: "More ways to start" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "New world…" }));
  const dialog = await screen.findByRole("dialog", { name: "Start new world" });
  // The last world's options come back.
  expect(within(dialog).getByRole("checkbox", { name: "Start with the pogo stick" })).toBeChecked();
  const ammo = within(dialog).getByRole("spinbutton", { name: "Starting ammo" });
  expect(ammo).toHaveValue(15);
  const seed = within(dialog).getByRole("textbox", { name: "Seed" });
  await userEvent.type(seed, "70000");
  expect(within(dialog).getByText("A whole number from 0 to 65535.")).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Start" })).toBeDisabled();
  await userEvent.clear(seed);
  await userEvent.type(seed, "12345");
  await userEvent.clear(ammo);
  await userEvent.type(ammo, "20");
  await userEvent.click(within(dialog).getByRole("button", { name: "Start" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenLastCalledWith(
      "keen4-1",
      {
        type: "new",
        seed: 12345,
        options: { startpogo: 1, startammo: 20 },
      },
      null,
      "en",
    ),
  );
});

test("the arrow beside Continue offers five worlds, favourites first, each by last start", async () => {
  withWorlds();
  const others = Array.from({ length: 6 }, (_, i) => ({
    ...randomiser.worlds[1],
    id: 10 + i,
    name: `World ${i}`,
    lastPlayedAt: 1_690_000_000 + i,
    favourite: i === 0 || i === 2,
  }));
  vi.mocked(commands.profileWorlds).mockResolvedValue(
    ok({ ...randomiser, worlds: [randomiser.worlds[0], ...others] }),
  );
  renderDetail();
  await screen.findByRole("button", { name: "Continue world" });
  await vi.waitFor(() => expect(commands.profileWorlds).toHaveBeenCalled());
  await userEvent.click(screen.getByRole("button", { name: "More ways to start" }));
  const items = await screen.findAllByRole("menuitem");
  // The world Continue starts isn't repeated; the oldest non-favourite is left out.
  expect(items.map((i) => i.textContent)).toEqual([
    "New world…",
    "World 2",
    "World 0",
    "World 5",
    "World 4",
    "World 3",
  ]);
  await userEvent.click(items[2]);
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      { type: "continue", id: 10 },
      null,
      "en",
    ),
  );
});

test("without a world going on, the arrow beside New world offers the others", async () => {
  withWorlds(null);
  vi.mocked(commands.profileWorlds).mockResolvedValue(
    ok({
      ...randomiser,
      worlds: randomiser.worlds.map((w) => ({ ...w, active: false })),
      active: null,
    }),
  );
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "More ways to start" }));
  const items = await screen.findAllByRole("menuitem");
  expect(items.map((i) => i.textContent)).toEqual(["Seed 4711", "Hard one"]);
  await userEvent.click(items[1]);
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      { type: "continue", id: 1 },
      null,
      "en",
    ),
  );
  await userEvent.click(screen.getByRole("button", { name: "New world…" }));
  expect(await screen.findByRole("dialog", { name: "Start new world" })).toBeInTheDocument();
});

test("a new world with a seed and options already played continues that one", async () => {
  withWorlds();
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "More ways to start" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "New world…" }));
  const dialog = await screen.findByRole("dialog", { name: "Start new world" });
  await userEvent.type(within(dialog).getByRole("textbox", { name: "Seed" }), "12");
  // "Hard one" was made with the defaults; the last options aren't those.
  expect(within(dialog).getByRole("button", { name: "Start" })).toBeInTheDocument();
  await userEvent.click(
    within(dialog).getByRole("checkbox", { name: "Start with the pogo stick" }),
  );
  const ammo = within(dialog).getByRole("spinbutton", { name: "Starting ammo" });
  await userEvent.clear(ammo);
  await userEvent.type(ammo, "0");
  expect(within(dialog).getByText(/is “Hard one”, which you already have/)).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Continue world" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      { type: "continue", id: 1 },
      null,
      "en",
    ),
  );
});

test("without a world played, Play begins one", async () => {
  withWorlds(null);
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "New world…" }));
  const dialog = await screen.findByRole("dialog", { name: "Start new world" });
  await userEvent.click(within(dialog).getByRole("button", { name: "Start" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      {
        type: "new",
        seed: null,
        options: { startpogo: 1, startammo: 15 },
      },
      null,
      "en",
    ),
  );
});

test("without worlds, the Worlds tab begins the first one", async () => {
  withWorlds(null);
  vi.mocked(commands.profileWorlds).mockResolvedValue(
    ok({ ...randomiser, worlds: [], active: null }),
  );
  renderDetail();
  const empty = (await screen.findByRole("heading", { name: "No worlds yet" })).parentElement!;
  const start = within(empty).getByRole("button", { name: "New world…" });
  await vi.waitFor(() => expect(start).toBeEnabled());
  await userEvent.click(start);
  const dialog = await screen.findByRole("dialog", { name: "Start new world" });
  await userEvent.click(within(dialog).getByRole("button", { name: "Start" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      expect.objectContaining({ type: "new" }),
      null,
      "en",
    ),
  );
});

test("a world can be made current without playing it", async () => {
  withWorlds();
  vi.mocked(commands.activateWorld).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /^Worlds/ }));
  // Greyed out for the one that is already, so the rows line up.
  await screen.findByRole("button", { name: "Make Hard one current without playing" });
  expect(screen.getByRole("button", { name: "Seed 4711 is already current" })).toBeDisabled();
  await userEvent.click(
    screen.getByRole("button", { name: "Make Hard one current without playing" }),
  );
  expect(commands.activateWorld).toHaveBeenCalledWith("keen4-1", 1);
  expect(await screen.findByText(/“Hard one” is current now/)).toBeInTheDocument();
  expect(commands.launchProfile).not.toHaveBeenCalled();
});

test("the Worlds tab lists the worlds and plays, renames and deletes them", async () => {
  withWorlds();
  vi.mocked(commands.setWorldFavourite).mockResolvedValue(ok(null));
  vi.mocked(commands.renameWorld).mockResolvedValue(ok(null));
  vi.mocked(commands.deleteWorld).mockResolvedValue(ok(null));
  renderDetail();
  // Counted, like saves.
  await userEvent.click(await screen.findByRole("tab", { name: "Worlds 2" }));
  const rows = await screen.findAllByRole("listitem");
  expect(rows[0]).toHaveTextContent("Seed 4711");
  expect(rows[0]).toHaveTextContent("Playing");
  expect(rows[0]).toHaveTextContent("Start with the pogo stick · Starting ammo: 15");
  expect(rows[0]).toHaveTextContent("2 saves");
  expect(rows[1]).toHaveTextContent("Hard one");
  // The seed is said once: in the name, else beside it.
  expect(rows[0]).not.toHaveTextContent("Seed: 4711");
  expect(rows[1]).toHaveTextContent("Seed: 12");

  await userEvent.click(screen.getByRole("button", { name: "Play Hard one" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith(
      "keen4-1",
      { type: "continue", id: 1 },
      null,
      "en",
    ),
  );
  await userEvent.click(screen.getByRole("button", { name: "Favourite: Hard one" }));
  expect(commands.setWorldFavourite).toHaveBeenCalledWith("keen4-1", 1, true);

  await userEvent.click(screen.getByRole("button", { name: "Rename Seed 4711" }));
  const rename = await screen.findByRole("dialog", { name: "Rename world" });
  await userEvent.type(within(rename).getByRole("textbox", { name: "Name" }), "Mine");
  await userEvent.click(within(rename).getByRole("button", { name: "Save" }));
  expect(commands.renameWorld).toHaveBeenCalledWith("keen4-1", 2, "Mine");

  await userEvent.click(screen.getByRole("button", { name: "Delete Hard one" }));
  const confirm = await screen.findByRole("dialog", { name: "Delete Hard one?" });
  await userEvent.click(within(confirm).getByRole("button", { name: "Delete" }));
  expect(commands.deleteWorld).toHaveBeenCalledWith("keen4-1", 1);
});

test("the Saves tab names the world whose saves it shows", async () => {
  withWorlds();
  vi.mocked(commands.listBackups).mockResolvedValue(ok([{ ...backup, world: 1 }]));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /Saves/ }));
  expect(await screen.findByText("Seed 4711")).toBeInTheDocument();
  expect(screen.getByText(/world Hard one/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "All worlds" }));
  expect(screen.getByRole("tab", { name: /^Worlds/ })).toHaveAttribute("aria-selected", "true");
});

function withVariants() {
  const variant = (id: string, name: string, de: string) => ({
    id,
    default: id === "hard",
    name,
    description: id === "easy" ? "Fewer enemies." : null,
    translations: { de: { name: de, description: null } },
    archive: null,
    root: null,
    recipe: null,
    copyToRoot: null,
    files: {},
    cover: null,
    fingerprint: null,
  });
  const hard = variant("hard", "Hard", "Schwer");
  const easy = variant("easy", "Easy", "Leicht");
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([
      {
        ...profile,
        modId: "clouds",
        label: "Clouds Unseen",
        variantCount: 2,
        activeVariant: easy,
        variantChosen: true,
      },
    ]),
  );
  vi.mocked(commands.listVariants).mockResolvedValue(
    ok({
      variants: [
        { variant: hard, active: false, saves: 1, lastPlayedAt: null },
        { variant: easy, active: true, saves: 2, lastPlayedAt: 1_700_000_000 },
      ],
      active: "easy",
      chosen: true,
      sameCovers: false,
    }),
  );
  vi.mocked(commands.launchProfile).mockResolvedValue(ok(null));
  vi.mocked(commands.setVariant).mockResolvedValue(ok(null));
  setControlsShown("galaxy");
}

test("a mod with variants plays the one chosen, or another from the arrow", async () => {
  withVariants();
  renderDetail();
  expect(await screen.findByText("2 variants")).toBeInTheDocument();
  // The variant played leads what it runs on; its description is in the tab.
  expect(screen.getByRole("button", { name: "Easy" }).parentElement).toHaveTextContent(
    /^Variant: Easy · Runs on/,
  );
  expect(screen.getAllByText("Fewer enemies.")).toHaveLength(1);
  await userEvent.click(await screen.findByRole("button", { name: "Play" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenCalledWith("keen4-1", null, null, "en"),
  );
  await userEvent.click(screen.getByRole("button", { name: "Other variants" }));
  await userEvent.click(screen.getByRole("menuitem", { name: "Hard" }));
  await vi.waitFor(() =>
    expect(commands.launchProfile).toHaveBeenLastCalledWith("keen4-1", null, "hard", "en"),
  );
});

test("the variant's name under the title leads to it in the Variants tab", async () => {
  withVariants();
  const scrolled: Element[] = [];
  Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
    scrolled.push(this);
  });
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Easy" }));
  expect(await screen.findByRole("tab", { name: "Variants 2" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await vi.waitFor(() => expect(scrolled).toHaveLength(1));
  expect(scrolled[0]).toHaveTextContent("Current");
  expect(scrolled[0]).toHaveTextContent("Easy");
});

test("the library card's link opens the Variants tab at the variant played", async () => {
  withVariants();
  const scrolled: Element[] = [];
  Element.prototype.scrollIntoView = vi.fn(function (this: Element) {
    scrolled.push(this);
  });
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ProfileDetail profileId="keen4-1" show="variant" />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("tab", { name: "Variants 2" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await vi.waitFor(() => expect(scrolled).toHaveLength(1));
  expect(scrolled[0]).toHaveTextContent("Easy");
});

test("the library card's saves link opens the Saves tab, also beside variants", async () => {
  withVariants();
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <ProfileDetail profileId="keen4-1" show="saves" />
    </QueryClientProvider>,
  );
  expect(await screen.findByRole("tab", { name: /^Saves/ })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});

test("the Readme tab follows the variant played", async () => {
  withVariants();
  const readme = (text: string) =>
    ok([{ file: "CLOUDS.TXT", text, html: false, pdf: false, word: false }]);
  vi.mocked(commands.modReadmes).mockResolvedValue(readme("Easy and hard, in black and white."));
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Readme" }));
  expect(await screen.findByText("Easy and hard, in black and white.")).toBeInTheDocument();

  // Hard is made current: the variant has its own documents.
  const listed = await commands.listProfiles();
  const variants = await commands.listVariants("keen4-1");
  if (listed.status !== "ok" || variants.status !== "ok") throw new Error("mocked above");
  const hard = variants.data!.variants[0].variant;
  vi.mocked(commands.listProfiles).mockResolvedValue(
    ok([{ ...listed.data[0], activeVariant: hard }]),
  );
  vi.mocked(commands.modReadmes).mockResolvedValue(readme("Now in green."));
  await userEvent.click(screen.getByRole("tab", { name: "Variants 2" }));
  await userEvent.click(
    await screen.findByRole("button", { name: "Make Hard current without playing" }),
  );
  await userEvent.click(screen.getByRole("tab", { name: "Readme" }));
  expect(await screen.findByText("Now in green.")).toBeInTheDocument();
});

test("a mod with variants opens on them, the first tab", async () => {
  withVariants();
  vi.mocked(commands.modReadmes).mockResolvedValue(
    ok([{ file: "README.TXT", text: "Hello.", html: false, pdf: false, word: false }]),
  );
  renderDetail();
  const tab = await screen.findByRole("tab", { name: "Variants 2" });
  expect(tab).toHaveAttribute("aria-selected", "true");
  expect(screen.getAllByRole("tab")[0]).toBe(tab);
  expect(screen.getAllByRole("tab")[1]).toHaveTextContent("Readme");
});

test("the Variants tab lists them and makes one current without playing", async () => {
  withVariants();
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: "Variants 2" }));
  const rows = await screen.findAllByRole("listitem");
  expect(rows[0]).toHaveTextContent("Hard");
  expect(rows[0]).toHaveTextContent("Default");
  expect(rows[0]).toHaveTextContent("Not played yet");
  expect(rows[1]).toHaveTextContent("Current");
  expect(rows[1]).not.toHaveTextContent("Default");
  expect(rows[1]).toHaveTextContent("2 saves");
  expect(screen.getByRole("button", { name: "Easy is already current" })).toBeDisabled();
  await userEvent.click(screen.getByRole("button", { name: "Make Hard current without playing" }));
  expect(commands.setVariant).toHaveBeenCalledWith("keen4-1", "hard");
  expect(await screen.findByText(/“Hard” is current now/)).toBeInTheDocument();
  expect(commands.launchProfile).not.toHaveBeenCalled();

  // The Saves tab says whose saves they are.
  await userEvent.click(screen.getByRole("tab", { name: /^Saves/ }));
  expect(await screen.findByText(/Saves of the variant/)).toHaveTextContent("Easy");
});

test("saves imported from a folder go into the variant or world played, as the dialog says", async () => {
  vi.mocked(openDialog).mockResolvedValue(String.raw`C:\Games\OLD`);
  vi.mocked(commands.folderSaves).mockResolvedValue(
    ok({ saves: [save("SAVEGAM0.CK4", 1, "old")], settings: 0, loose: false }),
  );
  withVariants();
  const { unmount } = renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /^Saves/ }));
  await userEvent.click(await screen.findByRole("button", { name: "Import from folder…" }));
  const dialog = await screen.findByRole("dialog", { name: "Import saves from “OLD”" });
  expect(
    within(dialog).getByText(
      "The saves go into the variant you’re playing, “Easy”. They have to be from that variant.",
    ),
  ).toBeInTheDocument();
  unmount();

  withWorlds();
  renderDetail();
  await userEvent.click(await screen.findByRole("tab", { name: /^Saves/ }));
  await userEvent.click(await screen.findByRole("button", { name: "Import from folder…" }));
  const worldDialog = await screen.findByRole("dialog", { name: "Import saves from “OLD”" });
  expect(
    within(worldDialog).getByText(/^The saves go into the world you’re playing, “.+”\./),
  ).toBeInTheDocument();
});
