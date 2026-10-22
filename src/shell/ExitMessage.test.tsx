import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, events, type PlaySessionEnded } from "../bindings";
import { ExitMessage } from "./ExitMessage";

type Listener = (e: { payload: PlaySessionEnded }) => void;
let listener: Listener | null = null;

vi.mock("../bindings", () => ({
  commands: { listProfiles: vi.fn() },
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

function ended(message: string | null) {
  act(() => listener!({ payload: { profileId: "pak-een", exitCode: 0, seconds: 3, message } }));
}

test("shows what the game printed when it ended", async () => {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <ExitMessage />
    </QueryClientProvider>,
  );
  expect(events.playSessionEnded.listen).toHaveBeenCalled();
  ended(null);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

  ended("Error: Unrecognised version of KEEN4C.EXE");
  const dialog = await screen.findByRole("dialog", { name: "Pak Een ended with a message" });
  expect(dialog).toHaveTextContent("Error: Unrecognised version of KEEN4C.EXE");
  await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
