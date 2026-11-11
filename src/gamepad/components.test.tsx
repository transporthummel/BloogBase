import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { Dialog } from "../ui/Dialog";
import { Menu } from "../ui/Menu";
import { Tabs } from "../ui/Tabs";
import { activate, back, move, switchTab } from "./actions";

function at(el: Element, x: number, y: number) {
  el.getBoundingClientRect = () => new DOMRect(x, y, 100, 30);
}

function TwoTabs() {
  const [tab, setTab] = useState<"a" | "b">("a");
  return (
    <Tabs
      label="Tabs"
      tabs={[
        { id: "a", label: "A" },
        { id: "b", label: "B" },
      ]}
      selected={tab}
      onSelect={setTab}
    >
      <button type="button">In {tab}</button>
    </Tabs>
  );
}

test("left and right switch tabs; down leaves the tab list", () => {
  render(<TwoTabs />);
  const a = screen.getByRole("tab", { name: "A" });
  at(a, 0, 0);
  at(screen.getByRole("tab", { name: "B" }), 120, 0);
  at(screen.getByRole("button", { name: "In a" }), 0, 60);
  act(() => a.focus());
  act(() => move("right"));
  const b = screen.getByRole("tab", { name: "B" });
  expect(b).toHaveFocus();
  expect(b).toHaveAttribute("aria-selected", "true");
  at(b, 120, 0);
  const inside = screen.getByRole("button", { name: "In b" });
  at(inside, 0, 60);
  act(() => move("down"));
  expect(inside).toHaveFocus();
});

test("a menu is walked, picked from and closed with the controller", async () => {
  const remove = vi.fn();
  render(
    <Menu
      label="More"
      trigger="…"
      items={[
        { label: "Open", onSelect: vi.fn() },
        { label: "Remove", onSelect: remove },
      ]}
    />,
  );
  const button = screen.getByRole("button", { name: "More" });
  await userEvent.setup().click(button);
  await vi.waitFor(() => expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus());
  act(() => move("down"));
  expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus();
  // Sideways there is nothing; the focus stays in the menu.
  act(() => move("right"));
  expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus();
  act(() => back(vi.fn()));
  expect(screen.queryByRole("menu")).toBeNull();
  expect(button).toHaveFocus();

  await userEvent.setup().click(button);
  await vi.waitFor(() => expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus());
  act(() => move("down"));
  act(() => activate());
  expect(remove).toHaveBeenCalledOnce();
});

test("B closes a dialog instead of going back", () => {
  const close = vi.fn();
  render(
    <Dialog title="Dialog" onClose={close}>
      <p>Text</p>
    </Dialog>,
  );
  const goBack = vi.fn();
  act(() => back(goBack));
  expect(close).toHaveBeenCalledOnce();
  expect(goBack).not.toHaveBeenCalled();
});

function PlayAndTabs() {
  return (
    <>
      <button type="button">Play</button>
      <TwoTabs />
    </>
  );
}

test("LB/RB switch tabs, wrapping; the focus stays on Play", () => {
  render(<PlayAndTabs />);
  const play = screen.getByRole("button", { name: "Play" });
  act(() => play.focus());
  act(() => switchTab(1));
  expect(screen.getByRole("tab", { name: "B" })).toHaveAttribute("aria-selected", "true");
  expect(play).toHaveFocus();
  act(() => switchTab(1));
  expect(screen.getByRole("tab", { name: "A" })).toHaveAttribute("aria-selected", "true");
  expect(play).toHaveFocus();
});

test("LB/RB take the focus along from the tab list or the panel", () => {
  render(<TwoTabs />);
  act(() => screen.getByRole("button", { name: "In a" }).focus());
  act(() => switchTab(-1));
  const b = screen.getByRole("tab", { name: "B" });
  expect(b).toHaveAttribute("aria-selected", "true");
  expect(b).toHaveFocus();
  act(() => switchTab(1));
  expect(screen.getByRole("tab", { name: "A" })).toHaveFocus();
});

test("LB/RB leave the page's tabs alone while a dialog is open", () => {
  render(
    <>
      <TwoTabs />
      <Dialog title="Sure?" onClose={() => {}}>
        <button type="button">OK</button>
      </Dialog>
    </>,
  );
  act(() => switchTab(1));
  expect(screen.getByRole("tab", { name: "A", hidden: true })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});
