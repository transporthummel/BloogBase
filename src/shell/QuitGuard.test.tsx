import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, events } from "../bindings";
import { QuitGuard } from "./QuitGuard";

type Listener = (e: { payload: { games: string[] } }) => void;
let listener: Listener | null = null;

vi.mock("../bindings", () => ({
  commands: { quitApp: vi.fn() },
  events: {
    quitRequested: {
      listen: vi.fn((f: Listener) => {
        listener = f;
        return Promise.resolve(() => {});
      }),
    },
  },
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.quitApp).mockResolvedValue(undefined as never);
});

function renderGuard() {
  render(
    <QueryClientProvider client={new QueryClient()}>
      <QuitGuard />
    </QueryClientProvider>,
  );
  expect(events.quitRequested.listen).toHaveBeenCalled();
  act(() => listener!({ payload: { games: ["Commander Keen 4", "Aurora Bliss"] } }));
}

test("closing while games run asks first and can stop them", async () => {
  renderGuard();
  const user = userEvent.setup();
  expect(screen.getByRole("dialog", { name: "Games are still running" })).toHaveTextContent(
    "Commander Keen 4, Aurora Bliss are still running. If you close BloogBase now, their play time can’t be tracked properly.",
  );
  expect(screen.getByText(/only the time played so far counts/)).toBeInTheDocument();
  await user.click(screen.getByRole("checkbox", { name: "Quit the games too" }));
  await user.click(screen.getByRole("button", { name: "Close BloogBase" }));
  expect(commands.quitApp).toHaveBeenCalledWith(true);
});

test("cancelling keeps BloogBase open", async () => {
  renderGuard();
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(commands.quitApp).not.toHaveBeenCalled();
});
