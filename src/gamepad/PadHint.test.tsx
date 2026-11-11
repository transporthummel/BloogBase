import { act, render, screen } from "@testing-library/react";
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
