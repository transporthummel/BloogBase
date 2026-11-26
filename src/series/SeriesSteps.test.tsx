import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { galaxy, toast } from "../test/series";
import { entry, game, indexMod, profile } from "../test/seriesData";
import { seriesOf, seriesPlaces } from "./series";
import { SeriesLine } from "./SeriesLine";
import { SeriesSteps } from "./SeriesSteps";

vi.mock("@tanstack/react-router", async () => ({
  Link: (await import("../test/TestLink")).TestLink,
}));
vi.mock("../bindings", () => ({
  commands: { listGames: vi.fn(), listProfiles: vi.fn(), indexMods: vi.fn() },
}));

const places = seriesPlaces([toast, galaxy]);
const NBSP = String.fromCharCode(0xa0);

function renderSteps(key: Parameters<typeof seriesOf>[1]) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SeriesSteps place={seriesOf(places, key)!} />
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
      profile("keys-mod", { modId: "keys-mod" }),
      profile("desert-mod", { modId: "desert-mod" }),
      // Two copies of Keen 5: the one played last opens.
      profile("keen5-a", { gameId: "keen5", lastPlayedAt: 10 }),
      profile("keen5-b", { gameId: "keen5", lastPlayedAt: 20 }),
      profile("keen4-a", { gameId: "keen4" }),
    ],
  });
});

test("leads to the parts before and after", async () => {
  renderSteps({ indexId: "desert" });
  expect(
    await screen.findByRole("link", { name: "Previous part: The Keys of Krodacia" }),
  ).toHaveAttribute("href", "/profiles/keys-mod");
  // Not installed: its card on the series' page, and the arrow says so.
  expect(
    screen.getByRole("link", { name: "Next part: Battle of the Brains (Not in your library)" }),
  ).toHaveAttribute("href", "/series/toast#part-mod-brains");
});

test("a game opens the copy played last; the first part has no step back", async () => {
  renderSteps({ gameId: "keen4" });
  expect(
    await screen.findByRole("link", { name: "Next part: The Armageddon Machine" }),
  ).toHaveAttribute("href", "/profiles/keen5-b");
  // The dimmed arrow is no link and nothing for screen readers.
  expect(screen.getAllByRole("link")).toHaveLength(1);
});

test("the last episode steps on to the bonus parts", async () => {
  renderSteps({ indexId: "brains" });
  expect(
    await screen.findByRole("link", { name: "Next part: Desert Levelpack (Not in your library)" }),
  ).toHaveAttribute("href", "/series/toast#part-mod-desert-pack");
});

test("the series' line counts the episodes when asked to, but not bonus parts", () => {
  const { rerender } = render(
    <SeriesLine place={seriesOf(places, { indexId: "desert" })!} count />,
  );
  // The number keeps to the name with no-break spaces.
  const text = () => screen.getByRole("paragraph").textContent?.split(NBSP).join(" ");
  expect(text()).toBe("The Universe is Toast! · 2/3, Part 2 of 3");
  rerender(<SeriesLine place={seriesOf(places, { indexId: "desert-pack" })!} count />);
  expect(text()).toBe("The Universe is Toast! · Bonus, Bonus");
});
