import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type GameKeys, type GameSettings } from "../bindings";
import { back, move } from "../gamepad/actions";
import { KeysDialog } from "./KeysDialog";

vi.mock("../bindings", () => ({
  commands: { gameSettings: vi.fn(), setGameSettings: vi.fn() },
}));

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const noKeys: GameKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};
const settings = (keys: Partial<GameKeys> = {}): GameSettings => ({
  sound: false,
  music: null,
  device: null,
  scoreBox: null,
  keys: { ...noKeys, ...keys },
});

const onClose = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.setGameSettings).mockResolvedValue(ok(null));
});

async function open(keys: Partial<GameKeys> = {}) {
  vi.mocked(commands.gameSettings).mockResolvedValue(ok(settings(keys)));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <KeysDialog onClose={onClose} />
    </QueryClientProvider>,
  );
  await screen.findByText("Jump");
}

async function pressFor(name: string, key: string, code: string) {
  await userEvent.click(screen.getByRole("button", { name: `Change ${name}` }));
  fireEvent.keyDown(document, { key, code });
}

test("pressing a key stores it", async () => {
  await open();
  await pressFor("Jump", "w", "KeyW");
  expect(commands.setGameSettings).toHaveBeenCalledWith(settings({ jump: 0x11 }));
});

test("Esc cancels waiting for a key and leaves the dialog open", async () => {
  await open();
  await pressFor("Jump", "Escape", "Escape");
  expect(commands.setGameSettings).not.toHaveBeenCalled();
  expect(onClose).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Change Jump" })).toBeInTheDocument();
});

test("the controller's D-pad gives no key, its B cancels", async () => {
  await open();
  await userEvent.click(screen.getByRole("button", { name: "Change Jump" }));
  move("down");
  expect(commands.setGameSettings).not.toHaveBeenCalled();
  back(vi.fn());
  expect(onClose).not.toHaveBeenCalled();
  expect(await screen.findByRole("button", { name: "Change Jump" })).toBeInTheDocument();
  // Waiting has ended: a key now gives nothing.
  fireEvent.keyDown(document, { key: "w", code: "KeyW" });
  expect(commands.setGameSettings).not.toHaveBeenCalled();
});

test("a reserved key says so", async () => {
  await open();
  await pressFor("Jump", "F1", "F1");
  expect(commands.setGameSettings).not.toHaveBeenCalled();
  expect(screen.getByText(/the games use it themselves/)).toBeInTheDocument();
});

test("a key DOS doesn't have says so", async () => {
  await open();
  await pressFor("Jump", "Meta", "MetaLeft");
  expect(commands.setGameSettings).not.toHaveBeenCalled();
  expect(screen.getByText(/DOS doesn't know this key/)).toBeInTheDocument();
});

test("a key taken by another action is refused", async () => {
  await open({ fire: 0x11 });
  await pressFor("Jump", "w", "KeyW");
  expect(commands.setGameSettings).not.toHaveBeenCalled();
  expect(screen.getByText(/already set for Fire/)).toBeInTheDocument();
});

test("a game's default on another action warns, but is stored", async () => {
  await open();
  await pressFor("Jump", " ", "Space");
  await waitFor(() =>
    expect(commands.setGameSettings).toHaveBeenCalledWith(settings({ jump: 0x39 })),
  );
  expect(screen.getByText(/also does something else in the games/)).toBeInTheDocument();
});

test("reset goes back to the game's key", async () => {
  await open({ jump: 0x11 });
  await userEvent.click(screen.getByRole("button", { name: "Reset Jump" }));
  expect(commands.setGameSettings).toHaveBeenCalledWith(settings());
});

test("one click resets all keys and nothing else", async () => {
  await open({ jump: 0x11, fire: 0x13 });
  await userEvent.click(screen.getByRole("button", { name: "Reset all" }));
  expect(commands.setGameSettings).toHaveBeenCalledWith(settings());
});

test("reset all only shows when a key is set", async () => {
  await open();
  expect(screen.queryByRole("button", { name: "Reset all" })).not.toBeInTheDocument();
});

test("focus stays in the dialog after choosing a key", async () => {
  await open();
  const change = screen.getByRole("button", { name: "Change Jump" });
  await pressFor("Jump", "w", "KeyW");
  expect(change).toHaveFocus();
});

test("focus stays in the dialog after a reset", async () => {
  await open({ jump: 0x11 });
  await userEvent.click(screen.getByRole("button", { name: "Reset Jump" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Change Jump" })).toHaveFocus());
});

test("focus stays in the dialog after resetting all keys", async () => {
  await open({ jump: 0x11 });
  await userEvent.click(screen.getByRole("button", { name: "Reset all" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Change Up" })).toHaveFocus());
});
