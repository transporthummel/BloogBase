import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { QuitButton } from "./QuitButton";

vi.mock("../bindings", () => ({
  commands: { closeApp: vi.fn(() => Promise.resolve({ status: "ok", data: null })) },
}));

test("the button closes BloogBase the way its window does", async () => {
  render(<QuitButton />);
  await userEvent.click(screen.getByRole("button", { name: "Quit BloogBase" }));
  expect(commands.closeApp).toHaveBeenCalledOnce();
});
