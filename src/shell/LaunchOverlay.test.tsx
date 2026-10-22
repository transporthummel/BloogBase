import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type GameLaunch, type LaunchPhase, type PlaySessionEnded } from "../bindings";
import { LaunchOverlay } from "./LaunchOverlay";

const listeners: {
  launch?: (e: { payload: GameLaunch }) => void;
  ended?: (e: { payload: PlaySessionEnded }) => void;
} = {};

vi.mock("../bindings", () => ({
  commands: { listProfiles: vi.fn(), listGames: vi.fn(), profileCover: vi.fn() },
  events: {
    gameLaunch: {
      listen: vi.fn((f) => {
        listeners.launch = f;
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
  vi.clearAllMocks();
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [{ id: "pak-een", label: "Pak Een", gameId: "keen4", installId: "keen4-1" }],
  } as never);
  vi.mocked(commands.listGames).mockResolvedValue([] as never);
  vi.mocked(commands.profileCover).mockResolvedValue({ status: "ok", data: null });
});

function launch(phase: LaunchPhase) {
  act(() => listeners.launch!({ payload: { profileId: "pak-een", phase } }));
}

function renderOverlay() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <LaunchOverlay />
    </QueryClientProvider>,
  );
}

test("covers the start until the window is up", async () => {
  renderOverlay();
  launch("starting");
  expect(await screen.findByRole("status")).toHaveTextContent("Starting Pak Een…");
  launch("shown");
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});

test("goes when the game fails or ends before, or on a click", async () => {
  renderOverlay();
  launch("starting");
  await screen.findByRole("status");
  launch("failed");
  expect(screen.queryByRole("status")).not.toBeInTheDocument();

  launch("starting");
  await screen.findByRole("status");
  act(() =>
    listeners.ended!({
      payload: { profileId: "pak-een", exitCode: 1, seconds: 0, message: null },
    }),
  );
  expect(screen.queryByRole("status")).not.toBeInTheDocument();

  launch("starting");
  fireEvent.mouseDown(await screen.findByRole("status"));
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
});
