import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type Install,
  type Mod,
  type ModInfo,
  type Profile,
  type RecipeOption,
  type WikiMod,
} from "../bindings";
import { AddModDialog } from "./AddModDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: {
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    listMods: vi.fn(),
    listProfiles: vi.fn(),
    inspectMod: vi.fn(),
    inspectDropped: vi.fn(),
    importGames: vi.fn(),
    installMod: vi.fn(),
    lookupKeenwiki: vi.fn(),
    downloadKeenwikiMod: vi.fn(),
    indexMods: vi.fn(),
    listJobs: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({
  Link: (props: { to: string }) => <a href={props.to} {...props} />,
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
  versions: [
    { id: "keen4-1.4-ega", label: "v1.4 EGA", files: {}, shareware: false, original: null },
  ],
};

const install: Install = {
  id: "keen4-1",
  gameId: "keen4",
  versionId: "keen4-1.4-ega",
  label: keen4.title,
  source: "folder",
  path: "/x",
  addedAt: 0,
};

const patch: RecipeOption = {
  recipe: { type: "patch", patcher: "CK4PATCH.EXE", patch: "abiathar.pat", args: [] },
  origin: "play.bat",
  gameVersions: ["1.4"],
  versionIds: ["keen4-1.4-ega"],
};

const info: ModInfo = {
  name: "Aurora Bliss",
  authors: ["dave216"],
  version: "1.2",
  released: "2023-08-23",
  gameId: "keen4",
  recipes: [patch],
  confident: true,
  fileCount: 18,
};

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

function renderDialog(
  client = new QueryClient(),
  replacing?: string,
  onGamesImported?: (count: number) => void,
) {
  const onInstalled = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <AddModDialog
        replacing={replacing}
        onClose={() => {}}
        onInstalled={onInstalled}
        onGamesImported={onGamesImported}
      />
    </QueryClientProvider>,
  );
  return onInstalled;
}

beforeEach(() => {
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install]));
  vi.mocked(commands.listMods).mockReturnValue(ok([]));
  vi.mocked(commands.listProfiles).mockReturnValue(ok([]));
  vi.mocked(open).mockResolvedValue("/downloads/aurora.zip");
  // Whatever a test makes inspectMod find is what a pick holds.
  vi.mocked(commands.inspectDropped).mockImplementation(async (path) => {
    const found = await commands.inspectMod(path);
    return found.status === "ok" ? ok({ kind: "mod" as const, info: found.data }) : found;
  });
});

test("imports games picked instead of a mod", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "games" }));
  vi.mocked(commands.importGames).mockReturnValue(ok([install, { ...install, id: "keen4-2" }]));
  const onGamesImported = vi.fn();
  renderDialog(new QueryClient(), undefined, onGamesImported);
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose folder…" }));

  await vi.waitFor(() => expect(onGamesImported).toHaveBeenCalledWith(2));
  expect(commands.importGames).toHaveBeenCalledWith("/downloads/aurora.zip");
  expect(commands.inspectMod).not.toHaveBeenCalled();
});

test("says so when a pick holds neither a game nor a mod", async () => {
  vi.mocked(commands.inspectDropped).mockReturnValue(ok({ kind: "nothing" }));
  renderDialog();
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(
    await screen.findByText("Neither a Commander Keen game nor a mod was found there."),
  ).toBeInTheDocument();
});

test("inspects the picked ZIP and installs the mod on the matching game", async () => {
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.installMod).mockReturnValue(
    ok({
      id: "aurora-bliss",
      gameId: "keen4",
      name: "Aurora",
      authors: [],
      authorLinks: {},
      version: null,
      description: null,
      released: null,
      recipe: patch.recipe,
      source: "manual",
      indexId: null,
      keenwikiUrl: null,
      addedAt: 0,
      profileId: "aurora-bliss",
    }),
  );
  const onInstalled = renderDialog();
  const user = userEvent.setup();

  await user.click(screen.getByRole("button", { name: "Choose ZIP file…" }));
  const name = await screen.findByRole("textbox", { name: "Name" });
  expect(name).toHaveValue("Aurora Bliss");
  expect(screen.getByText(keen4.title)).toBeInTheDocument();
  expect(screen.getByText("by dave216 · version 1.2 · Aug 23, 2023")).toBeInTheDocument();
  // Only one way to start it: no choice shown.
  expect(screen.queryByText("We need your help")).not.toBeInTheDocument();

  await user.clear(name);
  await user.type(name, "Aurora");
  await user.click(screen.getByRole("button", { name: "Install" }));
  expect(commands.installMod).toHaveBeenCalledWith({
    path: "/downloads/aurora.zip",
    name: "Aurora",
    recipe: patch.recipe,
    installId: "keen4-1",
    keenwiki: null,
    replace: null,
  });
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
});

test("asks whether another version replaces the installed one", async () => {
  const older: Mod = {
    id: "aurora-bliss",
    gameId: "keen4",
    name: "Aurora Bliss",
    authors: [],
    authorLinks: {},
    version: "1.1",
    description: null,
    released: null,
    recipe: patch.recipe,
    source: "manual",
    indexId: null,
    keenwikiUrl: null,
    addedAt: 0,
    profileId: "aurora-bliss",
  };
  const other = { ...install, id: "keen4-2" };
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install, other]));
  vi.mocked(commands.listMods).mockReturnValue(
    ok([older, { ...older, id: "dreams", gameId: "kdreams", profileId: "dreams" }]),
  );
  vi.mocked(commands.listProfiles).mockReturnValue(
    ok([{ id: "aurora-bliss", installId: "keen4-2" } as Profile]),
  );
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.installMod).mockReturnValue(ok(older));
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, "invalidateQueries");
  const onInstalled = renderDialog(client);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(await screen.findByText("You already have this mod")).toBeInTheDocument();
  // Only the one for the same game, picked already.
  expect(screen.getAllByRole("radio", { name: /^Replace/ })).toHaveLength(1);
  expect(
    screen.getByRole("radio", { name: /Replace Aurora Bliss \(version 1\.1\)/ }),
  ).toBeChecked();
  await user.click(screen.getByRole("button", { name: "Replace" }));
  // On the copy the old version played on, not the first one.
  expect(commands.installMod).toHaveBeenCalledWith(
    expect.objectContaining({ replace: "aurora-bliss", installId: "keen4-2" }),
  );
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
  // The new version may have another title screen and README.
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["cover", "aurora-bliss"] });
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["readme", "aurora-bliss"] });

  vi.mocked(commands.installMod).mockClear();
  await user.click(screen.getByRole("radio", { name: "Keep it and install this one as well" }));
  await user.click(screen.getByRole("button", { name: "Install" }));
  expect(commands.installMod).toHaveBeenCalledWith(
    expect.objectContaining({ replace: null, installId: "keen4-1" }),
  );
});

test("replaces a mod with a local copy, whatever its name", async () => {
  const old = {
    id: "aurora-old",
    gameId: "keen4",
    name: "My Aurora",
    version: null,
    profileId: "aurora-old",
  } as Mod;
  vi.mocked(commands.listMods).mockReturnValue(ok([old]));
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.installMod).mockReturnValue(ok(old));
  const onInstalled = renderDialog(new QueryClient(), "aurora-old");
  const user = userEvent.setup();

  expect(screen.getByRole("dialog", { name: "Replace with another copy" })).toBeInTheDocument();
  // Only local copies here.
  expect(screen.queryByRole("textbox")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Choose ZIP file…" }));
  expect(await screen.findByRole("radio", { name: /^Replace My Aurora/ })).toBeChecked();
  await user.click(screen.getByRole("button", { name: "Replace" }));
  expect(commands.installMod).toHaveBeenCalledWith(
    expect.objectContaining({ path: "/downloads/aurora.zip", replace: "aurora-old" }),
  );
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
});

test("won't replace a mod with one for another game", async () => {
  vi.mocked(commands.listMods).mockReturnValue(
    ok([{ id: "k5", gameId: "keen5", name: "Keen 5 Mod", profileId: "k5" } as Mod]),
  );
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  renderDialog(new QueryClient(), "k5");
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Keen 5 Mod is a mod for keen5, but this one is for Commander Keen 4: Secret of the Oracle, so it can’t replace it.",
  );
  expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
});

test("asks which recipe to use when unsure", async () => {
  vi.mocked(commands.inspectMod).mockReturnValue(
    ok({
      ...info,
      confident: false,
      recipes: [
        {
          recipe: { type: "batch", file: "BTOWER.BAT" },
          origin: "BTOWER.BAT",
          gameVersions: [],
          versionIds: [],
        },
        {
          recipe: {
            type: "patch",
            patcher: "CK4PATCH.EXE",
            patch: "abiathar.pat",
            args: ["-nowait"],
          },
          origin: "BTRNOMUS.BAT",
          gameVersions: [],
          versionIds: [],
        },
      ],
    }),
  );
  renderDialog();
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(await screen.findByText("We need your help")).toBeInTheDocument();
  const radios = screen.getAllByRole("radio");
  expect(radios).toHaveLength(2);
  expect(radios[0]).toBeChecked();
  expect(screen.getByText("CK4PATCH.EXE abiathar.pat -nowait")).toBeInTheDocument();
});

test("asks before installing a mod without its game", async () => {
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.listInstalls).mockReturnValue(ok([{ ...install, gameId: "keen5" }]));
  vi.mocked(commands.installMod).mockReturnValue(ok({ id: "aurora" } as never));
  const onInstalled = renderDialog();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(
    await screen.findByText(
      "Commander Keen 4: Secret of the Oracle (v1.4) isn’t in your library. Install the mod anyway? You can play it once the game is added.",
    ),
  ).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Install" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Install anyway" }));
  expect(commands.installMod).toHaveBeenCalledWith(expect.objectContaining({ installId: null }));
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
});

test("warns that patch mods may not fit a modified game", async () => {
  const cracked = {
    id: "keen4-x",
    label: "v1.4 (no copy protection)",
    files: {},
    shareware: false,
    original: "keen4-1.4-ega",
  };
  vi.mocked(commands.listGames).mockResolvedValue([{ ...keen4, versions: [cracked] }]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([{ ...install, versionId: "keen4-x" }]));
  vi.mocked(commands.inspectMod).mockReturnValue(
    ok({ ...info, recipes: [{ ...patch, versionIds: ["keen4-1.4-ega", "keen4-x"] }] }),
  );
  renderDialog();
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(await screen.findByText(/This copy of the game was changed/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Install" })).toBeEnabled();
});

test("warns when a patch without version info meets a different version than most mods use", async () => {
  const v10 = { id: "keen4-1.0", label: "v1.0", files: {}, shareware: false, original: null };
  vi.mocked(commands.listGames).mockResolvedValue([
    { ...keen4, moddingVersion: "keen4-1.4-ega", versions: [...keen4.versions, v10] },
  ]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([{ ...install, versionId: "keen4-1.0" }]));
  vi.mocked(commands.inspectMod).mockReturnValue(
    ok({ ...info, recipes: [{ ...patch, gameVersions: [], versionIds: [] }] }),
  );
  renderDialog();
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(
    await screen.findByText(/Most mods for this episode are made for v1.4 EGA, but yours is v1.0/),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Install" })).toBeEnabled();
});

const page: WikiMod = {
  pageUrl: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss",
  name: "Aurora Bliss",
  gameId: "keen4",
  authors: ["Dave216"],
  authorLinks: { Dave216: "https://keenwiki.shikadi.net/wiki/User:Dave216" },
  status: "Complete",
  released: "2023-08-23",
  updated: null,
  levels: "18",
  discussion: "https://pckf.com/viewtopic.php?t=1",
  description: "A Keen 4 mod.",
  credits: [],
  downloads: [
    {
      url: "https://files.example/aurora_v1.2.zip",
      label: "newest",
      note: "v.1.2",
      version: "1.2",
    },
    { url: "https://files.example/aurora_v1.0.zip", label: "v.1.0", note: null, version: "1.0" },
  ],
};

async function lookUp(url = page.pageUrl) {
  const user = userEvent.setup();
  await user.type(screen.getByRole("textbox", { name: "Add a mod from KeenWiki" }), url);
  await user.click(screen.getByRole("button", { name: "Look up" }));
  return user;
}

test("installs a mod from a KeenWiki page with the page's details", async () => {
  vi.mocked(commands.lookupKeenwiki).mockReturnValue(
    ok({ pageUrl: page.pageUrl, indexId: null, page }),
  );
  vi.mocked(commands.downloadKeenwikiMod).mockReturnValue(
    ok({ path: "/cache/aurora_v1.0.zip", info: { ...info, name: "Aurora", authors: ["dave"] } }),
  );
  vi.mocked(commands.installMod).mockReturnValue(
    ok({
      id: "aurora-bliss",
      gameId: "keen4",
      name: "Aurora Bliss",
      authors: ["Dave216"],
      authorLinks: { Dave216: "https://keenwiki.shikadi.net/wiki/User:Dave216" },
      version: "1.0",
      description: page.description,
      released: page.released,
      recipe: patch.recipe,
      source: "keenwiki",
      indexId: null,
      keenwikiUrl: page.pageUrl,
      addedAt: 0,
      profileId: "aurora-bliss",
    }),
  );
  const onInstalled = renderDialog();
  const user = await lookUp(`  ${page.pageUrl}  `);

  expect(commands.lookupKeenwiki).toHaveBeenCalledWith(page.pageUrl);
  const card = await screen.findByRole("region", { name: "Aurora Bliss" });
  expect(card).toHaveTextContent("Complete");
  expect(card).toHaveTextContent("A Keen 4 mod.");
  const radios = screen.getAllByRole("radio");
  expect(radios[0]).toBeChecked();
  await user.click(screen.getByRole("radio", { name: "v.1.0" }));
  await user.click(screen.getByRole("button", { name: "Download and check" }));
  expect(commands.downloadKeenwikiMod).toHaveBeenCalledWith(
    "https://files.example/aurora_v1.0.zip",
    "keen4",
  );

  // The page's name and authors win over what the archive suggests.
  expect(await screen.findByRole("textbox", { name: "Name" })).toHaveValue("Aurora Bliss");
  expect(screen.getByText("by Dave216 · version 1.0 · Aug 23, 2023")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Install" }));
  expect(commands.installMod).toHaveBeenCalledWith({
    path: "/cache/aurora_v1.0.zip",
    name: "Aurora Bliss",
    recipe: patch.recipe,
    installId: "keen4-1",
    keenwiki: {
      pageUrl: page.pageUrl,
      downloadUrl: "https://files.example/aurora_v1.0.zip",
      gameId: "keen4",
      authors: ["Dave216"],
      authorLinks: { Dave216: "https://keenwiki.shikadi.net/wiki/User:Dave216" },
      version: "1.0",
      description: "A Keen 4 mod.",
      released: page.released,
    },
    replace: null,
  });
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
});

test("warns when the page and the files disagree about the game", async () => {
  vi.mocked(commands.lookupKeenwiki).mockReturnValue(
    ok({ pageUrl: page.pageUrl, indexId: null, page: { ...page, gameId: "keen5" } }),
  );
  vi.mocked(commands.downloadKeenwikiMod).mockReturnValue(ok({ path: "/c/a.zip", info }));
  renderDialog();
  const user = await lookUp();
  await user.click(await screen.findByRole("button", { name: "Download and check" }));

  expect(
    await screen.findByText(
      /The KeenWiki page says this mod is for keen5, but its files look like/,
    ),
  ).toBeInTheDocument();
});

test("offers the index entry when the link is in the index", async () => {
  vi.mocked(commands.listJobs).mockResolvedValue([]);
  vi.mocked(commands.lookupKeenwiki).mockReturnValue(
    ok({ pageUrl: page.pageUrl, indexId: "aurora-bliss", page: null }),
  );
  vi.mocked(commands.indexMods).mockReturnValue(
    ok([
      {
        entry: {
          id: "aurora-bliss",
          name: "Aurora Bliss",
          game: "keen4",
          requires: [],
          authors: ["Dave216"],
          version: "1.2",
          released: null,
          updated: null,
          added: "2026-10-17",
          type: "episode",
          description: null,
          links: { keenwiki: page.pageUrl, pckf: null, homepage: null },
          downloads: [{ version: "1.2", url: "https://x/a.zip", sha256: "00", size: 1 }],
          recipe: patch.recipe,
          tags: [],
          featured: false,
          minAppVersion: null,
        },
        modId: null,
        updateAvailable: false,
      },
    ]),
  );
  renderDialog();
  await lookUp();

  expect(await screen.findByText(/tested and ready to install/)).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Aurora Bliss" })).toBeInTheDocument();
  expect(commands.downloadKeenwikiMod).not.toHaveBeenCalled();
});

test("explains links that aren't KeenWiki pages", async () => {
  vi.mocked(commands.lookupKeenwiki).mockReturnValue(
    Promise.resolve({
      status: "error" as const,
      error: { kind: "notKeenWikiLink" as const, message: "x" },
    }),
  );
  renderDialog();
  await lookUp("https://example.com/mod");

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Please paste a link to a mod page on keenwiki.shikadi.net",
  );
});
