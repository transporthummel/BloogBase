import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { FullscreenButton } from "./FullscreenButton";

const resized: (() => void)[] = [];
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onResized: (f: () => void) => {
      resized.push(f);
      return Promise.resolve(() => {});
    },
  }),
}));

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

test("on a Mac it names ⌃⌘F and follows fullscreen switched by macOS", async () => {
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Macintosh)");
  renderButton();
  const button = await screen.findByRole("button", { name: "Fullscreen" });
  expect(button).toHaveAttribute("title", "Fullscreen (⌃⌘F)");
  // The menu's ⌃⌘F or the green button, without BloogBase.
  fullscreen = true;
  resized.forEach((f) => f());
  await screen.findByRole("button", { name: "Leave fullscreen" });
  vi.restoreAllMocks();
});
