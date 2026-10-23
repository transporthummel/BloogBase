import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { Menu } from "./Menu";

test("a menu is worked with the keyboard and gives focus back on close", async () => {
  const open = vi.fn();
  const remove = vi.fn();
  render(
    <Menu
      label="More actions"
      trigger="…"
      items={[
        { label: "Open", onSelect: open },
        { label: "Busy", onSelect: vi.fn(), disabled: true },
        { label: "Remove", onSelect: remove, danger: true, separated: true },
      ]}
    />,
  );
  const user = userEvent.setup();
  const button = screen.getByRole("button", { name: "More actions" });
  await user.click(button);
  expect(button).toHaveAttribute("aria-expanded", "true");
  await vi.waitFor(() => expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus());
  // Past the disabled one, and around.
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("menuitem", { name: "Remove" })).toHaveFocus();
  await user.keyboard("{ArrowDown}");
  expect(screen.getByRole("menuitem", { name: "Open" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("menu")).toBeNull();
  expect(button).toHaveFocus();

  await user.click(button);
  await user.click(await screen.findByRole("menuitem", { name: "Remove" }));
  expect(remove).toHaveBeenCalled();
  expect(open).not.toHaveBeenCalled();
  expect(screen.queryByRole("menu")).toBeNull();
});
