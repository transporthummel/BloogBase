import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Backup, type Game, type Profile } from "../bindings";
import { profilesKey } from "../library/queries";
import { EarlierBackupPrompt } from "./EarlierBackup";

vi.mock("../bindings", () => ({
  commands: {
    listProfiles: vi.fn(),
    listGames: vi.fn(),
    earlierBackup: vi.fn(),
    answerEarlierBackup: vi.fn(),
  },
}));

const ok = <T,>(data: T) => Promise.resolve({ status: "ok" as const, data });

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
    { id: "keen4-1.2-ega", label: "v1.2 EGA", files: {}, shareware: false, original: null },
  ],
};

const old: Profile = {
  id: "keen5-1",
  installId: "keen5-1",
  gameId: "keen5",
  versionId: null,
  modId: null,
  label: "Commander Keen 5",
  sessionCount: 0,
  playSeconds: 0,
  lastPlayedAt: null,
  addedAt: 1_690_000_000,
  saveCount: 0,
  running: false,
  missingGame: null,
  playedOver: null,
};

const added: Profile = {
  ...old,
  id: "keen4-2",
  installId: "keen4-2",
  gameId: "keen4",
  versionId: "keen4-1.4-ega",
  label: keen4.title,
  addedAt: 1_700_000_000,
};

const backup: Backup = {
  id: 7,
  profileId: "keen4-1",
  label: null,
  reason: "gameRemoved",
  createdAt: 1_695_000_000,
  files: ["SAVEGAM0.CK4", "SAVEGAM1.CK4"],
  saves: 2,
  settings: 0,
  other: 0,
  auto: true,
  size: 300,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listGames).mockResolvedValue([keen4]);
  vi.mocked(commands.listProfiles).mockReturnValue(ok([old]));
  vi.mocked(commands.earlierBackup).mockReturnValue(ok({ backup, versionId: "keen4-1.2-ega" }));
  vi.mocked(commands.answerEarlierBackup).mockReturnValue(ok(null));
});

function renderPrompt() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <EarlierBackupPrompt />
    </QueryClientProvider>,
  );
  return client;
}

/** Lets the prompt see the library as it is at start, then adds `added`. */
async function addProfile(client: QueryClient) {
  await vi.waitFor(() => expect(client.getQueryData(profilesKey)).toBeDefined());
  vi.mocked(commands.listProfiles).mockReturnValue(ok([old, added]));
  await act(() => client.invalidateQueries({ queryKey: profilesKey }));
}

test("asks once right after adding whether to restore the saves kept on removal", async () => {
  const client = renderPrompt();
  await addProfile(client);

  const dialog = await screen.findByRole("dialog", {
    name: `Restore the saves of ${keen4.title}?`,
  });
  expect(dialog).toHaveTextContent(/When it was removed on .*, 2 saves were backed up\./);
  await vi.waitFor(() => expect(dialog).toHaveTextContent("Backed up from v1.2 EGA."));
  // Profiles there at start aren't asked about.
  expect(commands.earlierBackup).toHaveBeenCalledTimes(1);
  expect(commands.earlierBackup).toHaveBeenCalledWith("keen4-2");

  await userEvent.click(screen.getByRole("button", { name: "Restore" }));
  expect(commands.answerEarlierBackup).toHaveBeenCalledWith("keen4-2", 7, true);
  await vi.waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  // Seen now, so a refresh doesn't ask again.
  await act(() => client.invalidateQueries({ queryKey: profilesKey }));
  expect(commands.earlierBackup).toHaveBeenCalledTimes(1);
});

test("starting fresh declines it", async () => {
  const client = renderPrompt();
  await addProfile(client);
  await userEvent.click(await screen.findByRole("button", { name: "Start fresh" }));
  expect(commands.answerEarlierBackup).toHaveBeenCalledWith("keen4-2", 7, false);
});

test("says nothing without a backup of saves", async () => {
  vi.mocked(commands.earlierBackup).mockReturnValue(
    ok({ backup: { ...backup, saves: 0, settings: 1 }, versionId: null }),
  );
  const client = renderPrompt();
  await addProfile(client);
  await vi.waitFor(() => expect(commands.earlierBackup).toHaveBeenCalled());
  await act(() => Promise.resolve());
  expect(screen.queryByRole("dialog")).toBeNull();
});
