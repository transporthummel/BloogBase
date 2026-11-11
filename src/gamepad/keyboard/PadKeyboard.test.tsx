import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { Dialog } from "../../ui/Dialog";
import { activate } from "../actions";
import type { PadAction } from "../pad";
import { PadKeyboard } from "./PadKeyboard";
import { closeKeyboard, keyboardAction } from "./store";

// jsdom has no layout to watch.
vi.stubGlobal(
  "ResizeObserver",
  class {
    observe() {}
    disconnect() {}
  },
);

afterEach(() => act(() => closeKeyboard()));

/** As `usePad` does: the keyboard first. */
function pad(...actions: PadAction[]) {
  for (const a of actions) act(() => void keyboardAction(a));
}

const games = () =>
  [...screen.getByRole("list", { name: "Games" }).children].map((li) => li.textContent);

function Search() {
  const [q, setQ] = useState("");
  const games = ["Keen 4", "Keen Dreams", "Bloogwaters"].filter((g) =>
    g.toLowerCase().includes(q.toLowerCase()),
  );
  return (
    <>
      <input type="search" aria-label="Search" value={q} onChange={(e) => setQ(e.target.value)} />
      <ul aria-label="Games">
        {games.map((g) => (
          <li key={g}>{g}</li>
        ))}
      </ul>
      <PadKeyboard />
    </>
  );
}

test("A on a search field opens the keyboard, and typing filters", () => {
  render(<Search />);
  const field = screen.getByRole("searchbox");
  act(() => field.focus());
  act(() => activate());
  expect(screen.getByRole("group", { name: "On-screen keyboard" })).toBeInTheDocument();
  expect(field).toHaveFocus();

  // From Q down to Z, right to B.
  pad("down", "down", "right", "right", "right", "right", "activate");
  expect(field).toHaveValue("b");
  expect(games()).toEqual(["Bloogwaters"]);
  pad("x");
  expect(field).toHaveValue("");
  expect(games()).toHaveLength(3);
});

test("Shift capitalises one letter, the caret moves, Y types a space", async () => {
  render(<Search />);
  const field = screen.getByRole("searchbox");
  act(() => field.focus());
  act(() => activate());
  pad("l3", "activate", "activate", "y", "lb", "lb", "activate");
  // Q (shifted), q, space, then two back: between the two letters.
  expect(field).toHaveValue("Qqq ");
  await userEvent.click(screen.getByRole("button", { name: "Shift" }));
  expect(screen.getByRole("button", { name: "Shift" })).toHaveAttribute("aria-pressed", "true");
});

test("B closes only the keyboard, also inside a dialog", () => {
  const onClose = vi.fn();
  render(
    <Dialog title="Back up" onClose={onClose}>
      <input aria-label="Label" data-autofocus />
      <PadKeyboard />
    </Dialog>,
  );
  const field = screen.getByRole("textbox", { name: "Label" });
  act(() => activate());
  pad("activate");
  let took = false;
  act(() => {
    took = keyboardAction("back");
  });
  expect(took).toBe(true);
  expect(screen.queryByRole("group", { name: "On-screen keyboard" })).toBeNull();
  expect(onClose).not.toHaveBeenCalled();
  expect(field).toHaveValue("q");
  expect(field).toHaveFocus();
  expect(keyboardAction("back")).toBe(false);
});

function Backup({ careful = false, onSubmit }: { careful?: boolean; onSubmit: () => void }) {
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        onSubmit();
      }}
    >
      <input
        aria-label="Label"
        maxLength={80}
        onKeyDown={(e) => {
          if (e.key === "Enter" && careful) e.preventDefault();
        }}
      />
      <PadKeyboard />
    </form>
  );
}

test("Menu (Done) submits the form, unless the field holds Enter back", () => {
  const onSubmit = vi.fn();
  const { unmount } = render(<Backup onSubmit={onSubmit} />);
  act(() => screen.getByRole("textbox").focus());
  act(() => activate());
  pad("activate", "menu");
  expect(onSubmit).toHaveBeenCalledOnce();
  expect(screen.queryByRole("group")).toBeNull();
  unmount();

  const held = vi.fn();
  render(<Backup careful onSubmit={held} />);
  act(() => screen.getByRole("textbox").focus());
  act(() => activate());
  pad("menu");
  expect(held).not.toHaveBeenCalled();
});

test("a real key closes it and the field keeps the focus", async () => {
  render(<Search />);
  const field = screen.getByRole("searchbox");
  act(() => field.focus());
  act(() => activate());
  await userEvent.keyboard("k");
  expect(screen.queryByRole("group")).toBeNull();
  expect(field).toHaveFocus();
  expect(field).toHaveValue("k");
});

test("a number field gets digits, and Done leaves it so it saves", () => {
  const onBlur = vi.fn();
  render(
    <>
      <input type="number" aria-label="Keep" defaultValue="1" onBlur={onBlur} />
      <PadKeyboard />
    </>,
  );
  const field = screen.getByRole("spinbutton");
  act(() => field.focus());
  act(() => activate());
  expect(screen.queryByRole("button", { name: "q" })).toBeNull();
  pad("right", "activate", "menu");
  expect(field).toHaveValue(12);
  expect(onBlur).toHaveBeenCalled();
});

test("leaving the field closes it", () => {
  render(
    <>
      <Search />
      <button type="button">Elsewhere</button>
    </>,
  );
  act(() => screen.getByRole("searchbox").focus());
  act(() => activate());
  act(() => screen.getByRole("button", { name: "Elsewhere" }).focus());
  expect(screen.queryByRole("group")).toBeNull();
});
