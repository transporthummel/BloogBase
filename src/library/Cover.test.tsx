import { render } from "@testing-library/react";
import { expect, test } from "vitest";
import type { Game } from "../bindings";
import { Cover } from "./Cover";

const keen4 = { id: "keen4", engine: "galaxy" } as Game;

test("a placeholder shows the episode on its engine's colour", () => {
  const { container } = render(<Cover gameId="keen4" game={keen4} />);
  expect(container.textContent).toBe("4");
  expect(container.firstElementChild).toHaveClass("bg-galaxy");
});

test("a source mod's placeholder doesn't claim the episode it's built on", () => {
  const { container } = render(<Cover gameId="keen4" game={keen4} standalone />);
  expect(container.textContent).toBe("");
  expect(container.firstElementChild).not.toHaveClass("bg-galaxy");
  expect(container.querySelector("svg")).not.toBeNull();
});
