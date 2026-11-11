import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { controllerConnected } from "../profile/controller";
import { PadHint } from "./PadHint";

vi.mock("../profile/controller", () => ({ controllerConnected: vi.fn() }));

let pads: unknown[] = [];

beforeEach(() => {
  pads = [];
  Object.defineProperty(navigator, "getGamepads", { value: () => pads, configurable: true });
  vi.spyOn(document, "hasFocus").mockReturnValue(true);
});

afterEach(() => {
  vi.restoreAllMocks();
});

test("asks for a press while only XInput sees the controller", async () => {
  vi.mocked(controllerConnected).mockResolvedValue(true);
  render(<PadHint />);
  expect(await screen.findByRole("status")).toHaveTextContent("Press a button on the controller");

  pads = [{ connected: true, mapping: "standard", buttons: [], axes: [] }];
  act(() => {
    window.dispatchEvent(new Event("gamepadconnected"));
  });
  expect(screen.queryByRole("status")).toBeNull();
});

test("says nothing without a controller", async () => {
  vi.mocked(controllerConnected).mockResolvedValue(false);
  render(<PadHint />);
  await act(async () => {});
  expect(controllerConnected).toHaveBeenCalled();
  expect(screen.queryByRole("status")).toBeNull();
});

test("goes softly: kept until its leaving animation ends", async () => {
  vi.mocked(controllerConnected).mockResolvedValue(true);
  render(<PadHint />);
  const hint = await screen.findByRole("status");
  pads = [{ connected: true, mapping: "standard", buttons: [], axes: [] }];
  act(() => {
    window.dispatchEvent(new Event("gamepadconnected"));
  });
  expect(hint).toBeInTheDocument();
  expect(hint).toHaveClass("bb-hint-out");
  // React listens for the prefixed name under jsdom, which has no AnimationEvent.
  fireEvent(hint, new Event("webkitAnimationEnd", { bubbles: true }));
  expect(hint).not.toBeInTheDocument();
});

// Last: closing it holds for the rest of the app's run, so for this file too.
test("the cross closes it until the app starts anew", async () => {
  vi.mocked(controllerConnected).mockResolvedValue(true);
  const { unmount } = render(<PadHint />);
  await userEvent.click(await screen.findByRole("button", { name: "Close hint" }));
  expect(screen.queryByRole("status")).toBeNull();
  unmount();

  render(<PadHint />);
  await act(async () => {});
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("button", { name: "Close hint" })).toBeNull();
});
