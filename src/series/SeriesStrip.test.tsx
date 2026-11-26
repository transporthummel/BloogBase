import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { galaxy, toast } from "../test/series";
import { entry, game, indexMod, profile } from "../test/seriesData";
import { seriesOf, seriesPlaces } from "./series";
import { SeriesStrip } from "./SeriesStrip";

vi.mock("@tanstack/react-router", async () => ({
  Link: (await import("../test/TestLink")).TestLink,
}));
vi.mock("../bindings", () => ({
  commands: { listGames: vi.fn(), listProfiles: vi.fn(), indexMods: vi.fn() },
}));

const places = seriesPlaces([toast, galaxy]);

function renderStrip(key: Parameters<typeof seriesOf>[1]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SeriesStrip place={seriesOf(places, key)!} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(commands.listGames).mockResolvedValue([
    game("keen4", "Commander Keen 4"),
    game("keen5", "Commander Keen 5"),
    game("keen6", "Commander Keen 6"),
  ]);
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      indexMod(entry("keys", "The Universe is Toast!: The Keys of Krodacia"), "keys-mod"),
      indexMod(entry("desert", "The Universe is Toast!: Dead in the Desert"), "desert-mod"),
      indexMod(entry("brains", "Battle of the Brains")),
      indexMod(entry("desert-pack", "Desert Levelpack")),
    ],
  });
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [
      profile("keys-mod", { modId: "keys-mod", playSeconds: 600 }),
      profile("desert-mod", { modId: "desert-mod" }),
      // Two copies of Keen 5: the one played last opens.
      profile("keen5-a", { gameId: "keen5", lastPlayedAt: 10, playSeconds: 60 }),
      profile("keen5-b", { gameId: "keen5", lastPlayedAt: 20 }),
      profile("keen4-a", { gameId: "keen4" }),
    ],
  });
});

test("says which part it is and leads to the parts before and after", async () => {
  renderStrip({ indexId: "desert" });
  const strip = await screen.findByRole("region", { name: "Parts of The Universe is Toast!" });
  expect(strip).toHaveTextContent("Part 2 of 3 · The Universe is Toast!");
  expect(
    await within(strip).findByRole("link", {
      name: "Previous part: The Keys of Krodacia",
    }),
  ).toHaveAttribute("href", "/profiles/keys-mod");
  // Not installed: its card on the series' page.
  expect(
    within(strip).getByRole("link", { name: "Next part: Battle of the Brains" }),
  ).toHaveAttribute("href", "/series/toast#part-mod-brains");
  expect(within(strip).getByRole("link", { name: "Whole series" })).toHaveAttribute(
    "href",
    "/series/toast",
  );
});

test("lists every part, the current one marked, played ones ticked, bonus parts last", async () => {
  renderStrip({ indexId: "desert" });
  const strip = await screen.findByRole("region", { name: "Parts of The Universe is Toast!" });
  await within(strip).findByText("(Played)");
  const items = within(strip)
    .getAllByRole("listitem")
    .map((li) => li.textContent);
  expect(items).toEqual([
    "1The Keys of Krodacia(Played)",
    "2Dead in the Desert",
    "3Battle of the Brains",
    // The divider before the bonus parts is no item for screen readers.
    "Desert Levelpack",
  ]);
  expect(within(strip).getByText("Dead in the Desert").closest("[aria-current]")).toHaveAttribute(
    "aria-current",
    "page",
  );
});

test("a game opens the copy played last", async () => {
  renderStrip({ gameId: "keen4" });
  const strip = await screen.findByRole("region", { name: "Parts of Commander Keen 4–6" });
  expect(strip).toHaveTextContent("Part 1 of 3 · Commander Keen 4–6");
  expect(
    await within(strip).findByRole("link", { name: "Next part: The Armageddon Machine" }),
  ).toHaveAttribute("href", "/profiles/keen5-b");
  // No copy of Keen 6: the series' page.
  expect(within(strip).getByRole("link", { name: /Commander Keen 6/ })).toHaveAttribute(
    "href",
    "/series/keen-4-6#part-game-keen6",
  );
  expect(within(strip).queryByRole("link", { name: /Previous part/ })).not.toBeInTheDocument();
});
