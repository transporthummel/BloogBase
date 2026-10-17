import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type Install,
  type ModInfo,
  type RecipeOption,
  type WikiMod,
} from "../bindings";
import { AddModDialog } from "./AddModDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: {
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    inspectMod: vi.fn(),
    installMod: vi.fn(),
    lookupKeenwiki: vi.fn(),
    downloadKeenwikiMod: vi.fn(),
    indexMods: vi.fn(),
    listJobs: vi.fn(),
  },
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const keen4: Game = {
  id: "keen4",
  title: "Commander Keen 4: Secret of the Oracle",
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

function renderDialog() {
  const onInstalled = vi.fn();
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AddModDialog onClose={() => {}} onInstalled={onInstalled} />
    </QueryClientProvider>,
  );
  return onInstalled;
}

beforeEach(() => {
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install]));
  vi.mocked(open).mockResolvedValue("/downloads/aurora.zip");
});

test("inspects the picked ZIP and installs the mod on the matching game", async () => {
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.installMod).mockReturnValue(
    ok({
      id: "aurora-bliss",
      gameId: "keen4",
      name: "Aurora",
      authors: [],
      version: null,
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
  expect(screen.getByText("by dave216 · version 1.2 · 2023-08-23")).toBeInTheDocument();
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
  });
  await vi.waitFor(() => expect(onInstalled).toHaveBeenCalled());
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

test("explains a missing game instead of installing", async () => {
  vi.mocked(commands.inspectMod).mockReturnValue(ok(info));
  vi.mocked(commands.listInstalls).mockReturnValue(ok([{ ...install, gameId: "keen5" }]));
  renderDialog();
  await userEvent.setup().click(screen.getByRole("button", { name: "Choose ZIP file…" }));

  expect(
    await screen.findByText(
      "This mod needs Commander Keen 4: Secret of the Oracle (v1.4). Add it from Steam, GOG or a folder first.",
    ),
  ).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
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
  await user.type(screen.getByRole("textbox", { name: "KeenWiki link" }), url);
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
      version: "1.0",
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
  expect(screen.getByText("by Dave216 · version 1.0 · 2023-08-23")).toBeInTheDocument();
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
      version: "1.0",
    },
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
