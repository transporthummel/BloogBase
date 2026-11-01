import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, within } from "@testing-library/react";
import { openUrl } from "@tauri-apps/plugin-opener";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, events, type PlaySessionEnded } from "../bindings";
import { ExitMessage } from "./ExitMessage";

type Listener = (e: { payload: PlaySessionEnded }) => void;
let listener: Listener | null = null;

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

vi.mock("../bindings", () => ({
  commands: { listProfiles: vi.fn(), modReport: vi.fn() },
  events: {
    playSessionEnded: {
      listen: vi.fn((f: Listener) => {
        listener = f;
        return Promise.resolve(() => {});
      }),
    },
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [{ id: "pak-een", label: "Pak Een" }],
  } as never);
});

function ended(message: string | null, startProblem = false) {
  act(() =>
    listener!({
      payload: { profileId: "pak-een", exitCode: 0, seconds: 3, message, startProblem },
    }),
  );
}

function renderIt() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ExitMessage />
    </QueryClientProvider>,
  );
}

test("shows what the game printed when it ended", async () => {
  renderIt();
  expect(events.playSessionEnded.listen).toHaveBeenCalled();
  ended(null);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

  ended("Error: Unrecognised version of KEEN4C.EXE");
  const dialog = await screen.findByRole("dialog", { name: "Pak Een ended with a message" });
  expect(dialog).toHaveTextContent("Error: Unrecognised version of KEEN4C.EXE");
  await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

test("a mod's first start that ended a few seconds in offers a report", async () => {
  vi.mocked(commands.modReport).mockResolvedValue({
    status: "ok",
    data: "### Mod doesn't start: Pak Een",
  });
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue();
  renderIt();
  ended(null, true);
  await screen.findByRole("dialog", { name: "Pak Een ended right away" });
  await user.click(screen.getByRole("button", { name: "Mod won’t start?" }));

  const dialog = await screen.findByRole("dialog", { name: "Report a problem with Pak Een" });
  expect(await within(dialog).findByText("### Mod doesn't start: Pak Een")).toBeInTheDocument();
  expect(commands.modReport).toHaveBeenCalledWith("pak-een");
  await user.click(within(dialog).getByRole("button", { name: "Copy and open an issue" }));
  expect(writeText).toHaveBeenCalledWith("### Mod doesn't start: Pak Een");
  expect(openUrl).toHaveBeenCalledWith(
    "https://github.com/transporthummel/BloogBase/issues/new?title=Mod%20doesn't%20start%3A%20Pak%20Een",
  );
});

test("a message with a start problem offers the report next to it", async () => {
  renderIt();
  ended("Error: Unrecognised version of KEEN4C.EXE", true);
  await screen.findByRole("dialog", { name: "Pak Een ended with a message" });
  expect(screen.getByRole("button", { name: "Mod won’t start?" })).toBeInTheDocument();
});

test("a message without a start problem has no report button", async () => {
  renderIt();
  ended("Error: Unrecognised version of KEEN4C.EXE");
  await screen.findByRole("dialog", { name: "Pak Een ended with a message" });
  expect(screen.queryByRole("button", { name: "Mod won’t start?" })).toBeNull();
});
