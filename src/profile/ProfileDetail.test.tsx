import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import {
  commands,
  type Backup,
  type Game,
  type Profile,
  type RuntimeStatus,
  type SaveFile,
} from "../bindings";
import { ProfileDetail } from "./ProfilePage";
import { slotFiles } from "./saves";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a>{children}</a>,
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
  ];
  return {
    commands: Object.fromEntries(names.map((n) => [n, vi.fn()])),
    events: { playSessionEnded: event },
  };
});

const keen4: Game = {
  id: "keen4",
  title: "Commander Keen 4: Secret of the Oracle",
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
  saveCount: 2,
  running: false,
};

const save = (file: string, slot: number, name: string | null): SaveFile => ({
  file,
  slot,
  modifiedAt: 1_700_000_000,
  size: 100,
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

function renderDetail() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
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
  expect(screen.getByText(/2 files/)).toBeInTheDocument();
});

test("deleting a save asks first", async () => {
  vi.mocked(commands.deleteSave).mockResolvedValue(ok(null));
  renderDetail();
  await userEvent.click(await screen.findByRole("button", { name: "Delete Slot 1 · pabba" }));
  const dialog = screen.getByRole("dialog");
  expect(within(dialog).getByText(/backed up first/)).toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
  expect(commands.deleteSave).toHaveBeenCalledWith("keen4-1", "SAVEGAM0.CK4");
  expect(await screen.findByText("Save deleted. A backup was kept.")).toBeInTheDocument();
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
  expect(within(dialog).getByText(/One other file/)).toBeInTheDocument();

  await userEvent.selectOptions(within(dialog).getByLabelText("Into"), "SAVEGAM2.CK4");
  expect(within(dialog).queryByText(/^Replaces/)).not.toBeInTheDocument();
  await userEvent.click(within(dialog).getByRole("button", { name: "Restore this save" }));
  expect(commands.restoreBackup).toHaveBeenCalledWith(7, "SAVEGAM0.CK4", "SAVEGAM2.CK4");
  expect(await screen.findByText(/^Restored/)).toBeInTheDocument();
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
