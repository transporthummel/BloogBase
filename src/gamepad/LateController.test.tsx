import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type ControllerTooLate, type PlaySessionEnded } from "../bindings";
import { LateController } from "./LateController";

const listeners: {
  late?: (e: { payload: ControllerTooLate }) => void;
  ended?: (e: { payload: PlaySessionEnded }) => void;
} = {};

vi.mock("../bindings", () => ({
  commands: { listProfiles: vi.fn() },
  events: {
    controllerTooLate: {
      listen: vi.fn((f) => {
        listeners.late = f;
        return Promise.resolve(() => {});
      }),
    },
    playSessionEnded: {
      listen: vi.fn((f) => {
        listeners.ended = f;
        return Promise.resolve(() => {});
      }),
    },
  },
}));

beforeEach(() => {
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [{ id: "keen4-1", label: "Keen 4", gameId: "keen4", installId: "keen4-1" }],
  } as never);
  render(
    <QueryClientProvider client={new QueryClient()}>
      <LateController />
    </QueryClientProvider>,
  );
});

const late = () => act(() => listeners.late!({ payload: { profileId: "keen4-1" } }));

test("a controller connected during a game asks for a restart, until the game ends", async () => {
  expect(screen.queryByRole("status")).toBeNull();
  late();
  expect(await screen.findByRole("status")).toHaveTextContent(
    "Keen 4 started before the controller was connected. Start the game again",
  );
  act(() =>
    listeners.ended!({
      payload: {
        profileId: "keen4-1",
        exitCode: 0,
        seconds: 60,
        message: null,
        startProblem: false,
      },
    }),
  );
  expect(screen.queryByRole("status")).toBeNull();
});

test("the cross closes it", async () => {
  late();
  await userEvent.click(await screen.findByRole("button", { name: "Close hint" }));
  expect(screen.queryByRole("status")).toBeNull();
});
