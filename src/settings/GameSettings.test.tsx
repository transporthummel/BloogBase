import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type GameSettings } from "../bindings";
import { ScoreBoxSection, SoundSection } from "./GameSettings";

vi.mock("../bindings", () => ({
  commands: { gameSettings: vi.fn(), setGameSettings: vi.fn() },
}));

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const noKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};
const unset: GameSettings = {
  sound: null,
  music: null,
  device: null,
  scoreBox: null,
  keys: noKeys,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.setGameSettings).mockResolvedValue(ok(null));
});

function renderSections(settings: GameSettings) {
  vi.mocked(commands.gameSettings).mockResolvedValue(ok(settings));
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SoundSection />
      <ScoreBoxSection />
    </QueryClientProvider>,
  );
}

test("everything starts as set by the game", async () => {
  renderSections(unset);
  for (const name of ["Sound", "Music", "Sound device", "Show score box"]) {
    expect(await screen.findByRole("combobox", { name })).toHaveDisplayValue("Set by the game");
  }
});

test("choosing a value saves all settings", async () => {
  const settings = { ...unset, music: true, keys: { ...noKeys, jump: 0x11 } };
  renderSections(settings);
  await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Sound" }), "Off");
  expect(commands.setGameSettings).toHaveBeenCalledWith({ ...settings, sound: false });
});

test("the device can't be chosen with sound off", async () => {
  renderSections({ ...unset, sound: false });
  expect(await screen.findByRole("combobox", { name: "Sound device" })).toBeDisabled();
});

test("choosing the device stores it", async () => {
  renderSections(unset);
  await userEvent.selectOptions(
    await screen.findByRole("combobox", { name: "Sound device" }),
    "PC Speaker",
  );
  expect(commands.setGameSettings).toHaveBeenCalledWith({ ...unset, device: "pcSpeaker" });
});

test("going back to the game's value stores nothing for it", async () => {
  renderSections({ ...unset, scoreBox: false });
  const select = await screen.findByRole("combobox", { name: "Show score box" });
  await waitFor(() => expect(select).toHaveDisplayValue("Off"));
  await userEvent.selectOptions(select, "Set by the game");
  expect(commands.setGameSettings).toHaveBeenCalledWith(unset);
});

test("the score box sits under Other", async () => {
  renderSections(unset);
  expect(await screen.findByRole("heading", { name: "Other" })).toBeInTheDocument();
});

test("a change right after another one keeps both", async () => {
  let finish = (_: ReturnType<typeof ok<null>>) => {};
  vi.mocked(commands.setGameSettings).mockReturnValueOnce(new Promise((r) => (finish = r)));
  renderSections(unset);
  await userEvent.selectOptions(await screen.findByRole("combobox", { name: "Sound" }), "Off");
  // Still saving; the other section has its own save.
  await userEvent.selectOptions(screen.getByRole("combobox", { name: "Show score box" }), "Off");
  finish(ok(null));
  await waitFor(() => expect(commands.setGameSettings).toHaveBeenCalledTimes(2));
  expect(commands.setGameSettings).toHaveBeenLastCalledWith({
    ...unset,
    sound: false,
    scoreBox: false,
  });
});
