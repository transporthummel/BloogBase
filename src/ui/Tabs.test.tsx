import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { afterEach, expect, test, vi } from "vitest";
import { fitTabs } from "./fitTabs";
import { Tabs } from "./Tabs";

test("tabs that don't fit give way, the selected one stays", () => {
  // Four tabs of 100 with gaps of 4: 412 for all.
  const widths = [100, 100, 100, 100];
  expect(fitTabs(widths, 0, 412, 60)).toEqual([0, 1, 2, 3]);
  // Two and "More" (60) take 268.
  expect(fitTabs(widths, 0, 300, 60)).toEqual([0, 1]);
  // The last one selected: it takes the place of the second.
  expect(fitTabs(widths, 3, 300, 60)).toEqual([0, 3]);
  // Too narrow for any but the selected one.
  expect(fitTabs(widths, 2, 120, 60)).toEqual([2]);
});

afterEach(() => vi.restoreAllMocks());

test("a tab picked from More comes into the line and takes the focus", async () => {
  // Every tab and "More" 100 wide, the row 320: two tabs and "More".
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(100);
  vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(320);
  function Page() {
    const [tab, setTab] = useState<"a" | "b" | "c" | "d">("a");
    return (
      <Tabs
        tabs={[
          { id: "a", label: "Alpha" },
          { id: "b", label: "Beta" },
          { id: "c", label: "Gamma", count: 3 },
          { id: "d", label: "Delta" },
        ]}
        selected={tab}
        onSelect={setTab}
        label="Parts"
      >
        {tab}
      </Tabs>
    );
  }
  render(<Page />);
  expect(screen.getAllByRole("tab").map((x) => x.textContent)).toEqual(["Alpha", "Beta"]);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "More" }));
  expect(screen.getByRole("menuitem", { name: "Gamma 3" })).toBeInTheDocument();
  await user.click(screen.getByRole("menuitem", { name: "Delta" }));
  expect(screen.getAllByRole("tab").map((x) => x.textContent)).toEqual(["Alpha", "Delta"]);
  expect(screen.getByRole("tab", { name: "Delta" })).toHaveFocus();
  expect(screen.getByRole("tabpanel")).toHaveTextContent("d");
  // The arrow keys go through the tabs in the line.
  await user.keyboard("{ArrowRight}");
  expect(screen.getByRole("tab", { name: "Alpha" })).toHaveFocus();
});
