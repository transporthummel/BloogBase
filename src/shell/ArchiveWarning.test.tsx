import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import type { ArchiveDamaged } from "../bindings";
import { ArchiveWarning } from "./ArchiveWarning";

type Listener = (e: { payload: ArchiveDamaged }) => void;
let listener: Listener | null = null;

vi.mock("../bindings", () => ({
  events: {
    archiveDamaged: {
      listen: vi.fn((f: Listener) => {
        listener = f;
        return Promise.resolve(() => {});
      }),
    },
  },
}));

test("warns about each import with damaged files in turn", async () => {
  render(<ArchiveWarning />);
  act(() => {
    listener!({ payload: { name: "Dopefish Levels", files: ["Level06.ck1"] } });
    listener!({ payload: { name: "Another Pack", files: ["A.CK4", "B.CK4"] } });
  });
  expect(screen.getByRole("dialog", { name: "A file may be damaged" })).toHaveTextContent(
    "Dopefish Levels was added, but this file in its ZIP failed its checksum",
  );
  expect(screen.getByText("Level06.ck1")).toBeInTheDocument();

  await userEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.getByRole("dialog", { name: "Some files may be damaged" })).toHaveTextContent(
    "Another Pack was added",
  );
  await userEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog")).toBeNull();
});
