import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { FullscreenButton } from "./FullscreenButton";

vi.mock("../bindings", () => ({
  commands: { windowFullscreen: vi.fn(), setWindowFullscreen: vi.fn() },
}));

let fullscreen = false;

beforeEach(() => {
  vi.clearAllMocks();
  fullscreen = false;
  vi.mocked(commands.windowFullscreen).mockImplementation(() => Promise.resolve(fullscreen));
  vi.mocked(commands.setWindowFullscreen).mockImplementation((on) => {
    fullscreen = on;
    return Promise.resolve({ status: "ok", data: null });
  });
});

function renderButton() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <FullscreenButton />
    </QueryClientProvider>,
  );
}

test("the button switches fullscreen on and off", async () => {
  renderButton();
  await userEvent.click(await screen.findByRole("button", { name: "Fullscreen" }));
  expect(commands.setWindowFullscreen).toHaveBeenCalledWith(true);
  await userEvent.click(await screen.findByRole("button", { name: "Leave fullscreen" }));
  expect(commands.setWindowFullscreen).toHaveBeenLastCalledWith(false);
});

test("F11 switches fullscreen too", async () => {
  renderButton();
  await screen.findByRole("button", { name: "Fullscreen" });
  await userEvent.keyboard("{F11}");
  expect(commands.setWindowFullscreen).toHaveBeenCalledWith(true);
  await screen.findByRole("button", { name: "Leave fullscreen" });
});
