import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Game, type Install, type ModInfo, type RecipeOption } from "../bindings";
import { AddModDialog } from "./AddModDialog";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: {
    listGames: vi.fn(),
    listInstalls: vi.fn(),
    inspectMod: vi.fn(),
    installMod: vi.fn(),
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
