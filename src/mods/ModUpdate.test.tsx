import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Game,
  type IndexMod,
  type Install,
  type Mod,
  type Recipe,
  type WikiMod,
} from "../bindings";
import { ModUpdate } from "./ModUpdate";
import { useModUpdateCheck } from "./modUpdateCheck";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: {
    checkModUpdate: vi.fn(),
    downloadKeenwikiMod: vi.fn(),
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    listMods: vi.fn(),
    listProfiles: vi.fn(),
    installMod: vi.fn(),
    indexMods: vi.fn(),
    refreshIndex: vi.fn(),
    installIndexMod: vi.fn(),
    listJobs: vi.fn(),
  },
}));

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

const patch: Recipe = { type: "patch", patcher: "CK4PATCH.EXE", patch: "a.pat", args: [] };

const mod: Mod = {
  id: "aurora-bliss",
  gameId: "keen4",
  name: "Aurora Bliss",
  authors: ["Dave216"],
  authorLinks: {},
  version: "1.1",
  description: null,
  released: null,
  recipe: patch,
  source: "keenwiki",
  indexId: null,
  extends: null,
  keenwikiUrl: "https://keenwiki.shikadi.net/wiki/Aurora_Bliss",
  homepage: null,
  pckf: null,
  keenmodding: null,
  addedAt: 0,
  profileId: "aurora-bliss",
};

const page: WikiMod = {
  pageUrl: mod.keenwikiUrl!,
  name: "Aurora Bliss",
  gameId: "keen4",
  authors: ["Dave216"],
  authorLinks: {},
  status: "Complete",
  released: null,
  updated: null,
  levels: null,
  pckf: null,
  keenmodding: null,
  homepage: null,
  description: null,
  credits: [],
  downloads: [
    { url: "https://files.example/aurora_v1.2.zip", label: "newest", note: null, version: "1.2" },
    { url: "https://files.example/aurora_v1.1.zip", label: "v.1.1", note: null, version: "1.1" },
  ],
};

const install: Install = {
  id: "keen4-1",
  gameId: "keen4",
  versionId: null,
  label: "Commander Keen 4",
  source: "folder",
  path: "/x",
  addedAt: 0,
};

/** The detail page asks from its menu. */
function Checked({ mod }: { mod: Mod }) {
  const check = useModUpdateCheck(mod);
  return (
    <>
      <button onClick={() => check.mutate()}>Check for updates</button>
      <ModUpdate mod={mod} check={check} disabled={false} />
    </>
  );
}

function renderUpdate(m = mod) {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <Checked mod={m} />
    </QueryClientProvider>,
  );
  return userEvent.setup();
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listGames).mockResolvedValue([
    { id: "keen4", title: "Commander Keen 4", versions: [] } as unknown as Game,
  ]);
  vi.mocked(commands.listInstalls).mockReturnValue(ok([install]));
  vi.mocked(commands.listMods).mockReturnValue(ok([mod]));
  vi.mocked(commands.listProfiles).mockReturnValue(ok([]));
  vi.mocked(commands.listJobs).mockResolvedValue([]);
  vi.mocked(commands.refreshIndex).mockResolvedValue(indexStatus);
});

const indexStatus = {
  url: "https://index.example/index.json",
  checkedAt: 1,
  generatedAt: 1,
  modCount: 1,
  hiddenCount: 0,
  error: null,
};

const indexMod = { ...mod, indexId: "aurora-bliss", source: "index" as const };

function inIndex(updateAvailable: boolean) {
  vi.mocked(commands.indexMods).mockReturnValue(
    ok([
      {
        entry: { id: "aurora-bliss", name: "Aurora Bliss", version: "1.2" },
        modId: "aurora-bliss",
        updateAvailable,
      } as IndexMod,
    ]),
  );
}

test("says when the newest version is installed", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(
    ok({ status: "upToDate", page, refreshed: [] }),
  );
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByText("You have the newest version.")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
  expect(commands.checkModUpdate).toHaveBeenCalledWith("aurora-bliss");
});

test("says what it took over from the KeenWiki page", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(
    ok({ status: "upToDate", page, refreshed: ["description", "pckf"] }),
  );
  const invalidate = vi.spyOn(QueryClient.prototype, "invalidateQueries");
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(
    await screen.findByText("Updated from the KeenWiki page: description, PCKF link."),
  ).toBeInTheDocument();
  expect(screen.getByText("You have the newest version.")).toBeInTheDocument();
  // The mod's page shows the new details.
  expect(invalidate).toHaveBeenCalledWith({ queryKey: ["mods"] });
  invalidate.mockRestore();
});

test("downloads a newer version and replaces the installed one", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(ok({ status: "newer", page, refreshed: [] }));
  vi.mocked(commands.downloadKeenwikiMod).mockReturnValue(
    ok({
      path: "/cache/aurora_v1.2.zip",
      info: {
        name: "aurora",
        authors: [],
        version: null,
        released: null,
        gameId: "keen4",
        recipes: [
          {
            recipe: patch,
            origin: "play.bat",
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
      },
    }),
  );
  vi.mocked(commands.installMod).mockReturnValue(ok({ ...mod, version: "1.2" }));
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByText("Version 1.2 is available on KeenWiki.")).toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "Update" }));
  expect(commands.downloadKeenwikiMod).toHaveBeenCalledWith(
    "https://files.example/aurora_v1.2.zip",
    "keen4",
  );
  expect(await screen.findByRole("dialog", { name: "Update a mod" })).toBeInTheDocument();
  expect(
    screen.getByRole("radio", { name: /Replace Aurora Bliss \(version 1\.1\)/ }),
  ).toBeChecked();
  await user.click(screen.getByRole("button", { name: "Replace" }));
  expect(commands.installMod).toHaveBeenCalledWith(
    expect.objectContaining({
      replace: "aurora-bliss",
      keenwiki: expect.objectContaining({
        downloadUrl: "https://files.example/aurora_v1.2.zip",
        version: "1.2",
      }),
    }),
  );
  await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

test("updates an index mod from a fresh index", async () => {
  inIndex(true);
  vi.mocked(commands.installIndexMod).mockResolvedValue({
    id: 1,
    kind: "mod",
    subject: "aurora-bliss",
    phase: "starting",
  } as never);
  const user = renderUpdate(indexMod);
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByText("Version 1.2 is in the BloogBase mod list.")).toBeInTheDocument();
  expect(commands.refreshIndex).toHaveBeenCalledWith(true);
  expect(commands.checkModUpdate).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: "Update" }));
  expect(commands.installIndexMod).toHaveBeenCalledWith("aurora-bliss", null, true, null);
});

test("an index mod without a KeenWiki page asks only the index", async () => {
  inIndex(false);
  const user = renderUpdate({ ...indexMod, keenwikiUrl: null });
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByText("You have the newest version.")).toBeInTheDocument();
  expect(commands.checkModUpdate).not.toHaveBeenCalled();
});

test("a failed index check can't tell the mod is up to date", async () => {
  inIndex(false);
  vi.mocked(commands.refreshIndex).mockResolvedValue({
    ...indexStatus,
    error: { kind: "network", message: "offline" },
  } as never);
  const user = renderUpdate({ ...indexMod, keenwikiUrl: null });
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(/internet connection/);
  expect(screen.queryByText("You have the newest version.")).toBeNull();
});

test("asks KeenWiki when the index has nothing newer, and warns it leaves the index", async () => {
  inIndex(false);
  vi.mocked(commands.checkModUpdate).mockReturnValue(
    ok({ status: "unknown", page, refreshed: [] }),
  );
  const user = renderUpdate(indexMod);
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(
    await screen.findByText(
      "KeenWiki offers “newest”. BloogBase can’t tell whether it is newer than yours.",
    ),
  ).toBeInTheDocument();
  expect(
    screen.getByText(
      "After this update, the mod no longer gets updates from the BloogBase mod list.",
    ),
  ).toBeInTheDocument();
});

test("offers a download BloogBase can't open in the browser", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(ok({ status: "newer", page, refreshed: [] }));
  vi.mocked(commands.downloadKeenwikiMod).mockResolvedValue({
    status: "error",
    error: { kind: "unsupportedDownload", message: "RAR archive" },
  });
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  await user.click(await screen.findByRole("button", { name: "Update" }));
  await user.click(await screen.findByRole("button", { name: "Download it in your browser" }));
  expect(openUrl).toHaveBeenCalledWith("https://files.example/aurora_v1.2.zip");
});

test("offers a link that leads to a web page in the browser", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(ok({ status: "newer", page, refreshed: [] }));
  vi.mocked(commands.downloadKeenwikiMod).mockResolvedValue({
    status: "error",
    error: { kind: "webPage", message: "" },
  });
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  await user.click(await screen.findByRole("button", { name: "Update" }));
  await user.click(await screen.findByRole("button", { name: "Open it in your browser" }));
  expect(openUrl).toHaveBeenCalledWith("https://files.example/aurora_v1.2.zip");
});

test("dismissing an offered update hides it", async () => {
  vi.mocked(commands.checkModUpdate).mockReturnValue(ok({ status: "newer", page, refreshed: [] }));
  const user = renderUpdate();
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  await user.click(await screen.findByRole("button", { name: "Dismiss" }));
  expect(screen.queryByText("Version 1.2 is available on KeenWiki.")).toBeNull();
  expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
});

test("dismissing the index's update hides it", async () => {
  inIndex(true);
  const user = renderUpdate(indexMod);
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  await user.click(await screen.findByRole("button", { name: "Dismiss" }));
  expect(screen.queryByText("Version 1.2 is in the BloogBase mod list.")).toBeNull();
  expect(commands.installIndexMod).not.toHaveBeenCalled();
});
