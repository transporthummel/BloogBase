import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { expect, test, vi } from "vitest";
import { Dialog } from "./Dialog";

function Page({ onClose }: { onClose: () => void }) {
  const [count, setCount] = useState(0);
  return (
    <>
      <button type="button">Outside</button>
      <Dialog title="Hello" onClose={onClose}>
        <button type="button" onClick={() => setCount(count + 1)}>
          First {count}
        </button>
        <button type="button">Last</button>
      </Dialog>
    </>
  );
}

test("keeps focus inside, survives re-renders and closes with Escape", async () => {
  const onClose = vi.fn();
  render(<Page onClose={() => onClose()} />);
  expect(screen.getByRole("dialog")).toHaveFocus();

  await userEvent.tab();
  const first = screen.getByRole("button", { name: /First/ });
  expect(first).toHaveFocus();
  // A re-render with a new onClose must not move focus back to the panel.
  await userEvent.keyboard("{Enter}");
  expect(screen.getByRole("button", { name: "First 1" })).toHaveFocus();

  await userEvent.tab();
  expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();
  await userEvent.tab();
  expect(first).toHaveFocus();
  await userEvent.tab({ shift: true });
  expect(screen.getByRole("button", { name: "Last" })).toHaveFocus();

  await userEvent.keyboard("{Escape}");
  expect(onClose).toHaveBeenCalledOnce();
});
