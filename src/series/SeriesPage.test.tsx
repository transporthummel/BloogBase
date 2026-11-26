import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { galaxy, toast } from "../test/series";
import { entry, game, indexMod, profile } from "../test/seriesData";
import { SeriesPage } from "./SeriesPage";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tanstack/react-router", async () => ({
  Link: (await import("../test/TestLink")).TestLink,
  useRouterState: () => "",
  useParams: vi.fn(),
}));
vi.mock("../bindings", () => ({
  commands: {
    listGames: vi.fn(),
    listProfiles: vi.fn(),
    listInstalls: vi.fn(),
    listJobs: vi.fn(() => Promise.resolve([])),
    indexMods: vi.fn(),
    indexSeries: vi.fn(),
    freeGames: vi.fn(),
    profileCover: vi.fn(() => Promise.resolve({ status: "ok", data: null })),
    installIndexMod: vi.fn(),
  },
}));

function renderPage(seriesId: string) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <SeriesPage seriesId={seriesId} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.mocked(commands.indexSeries).mockResolvedValue([toast, galaxy]);
  vi.mocked(commands.listGames).mockResolvedValue([
    game("keen4", "Commander Keen 4"),
    game("keen5", "Commander Keen 5"),
    game("keen6", "Commander Keen 6"),
  ]);
  vi.mocked(commands.listInstalls).mockResolvedValue({ status: "ok", data: [] });
  vi.mocked(commands.freeGames).mockResolvedValue({
    status: "ok",
    data: [{ gameId: "keen6", versionId: "keen6-sw", setup: true, size: 1, installed: false }],
  });
  vi.mocked(commands.indexMods).mockResolvedValue({
    status: "ok",
    data: [
      indexMod(entry("keys", "The Universe is Toast!: The Keys of Krodacia"), "keys-mod"),
      indexMod(entry("desert", "Dead in the Desert", { authors: ["Gridlock"] })),
      indexMod(entry("brains", "Battle of the Brains")),
      indexMod(entry("desert-pack", "Desert Levelpack", { type: "levelpack" })),
    ],
  });
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [
      profile("keys-mod", { modId: "keys-mod", playSeconds: 600 }),
      profile("keen4-a", { gameId: "keen4" }),
    ],
  });
});

test("lists the episodes in order and the bonus parts, each to install or open", async () => {
  renderPage("toast");
  expect(
    await screen.findByRole("heading", { level: 1, name: "The Universe is Toast!" }),
  ).toBeInTheDocument();
  expect(screen.getByText("Keen against the Shikadi.")).toBeInTheDocument();
  expect(screen.getByText("by Someone, Gridlock")).toBeInTheDocument();
  expect(screen.getByText("1 of 4 played")).toBeInTheDocument();

  const episodes = screen.getByRole("region", { name: "Episodes" });
  const items = within(episodes).getAllByRole("listitem");
  expect(items.map((li) => within(li).getByRole("heading", { level: 3 }).textContent)).toEqual([
    "The Keys of Krodacia",
    "Dead in the Desert",
    "Battle of the Brains",
  ]);
  expect(items.map((li) => li.id)).toEqual(["part-mod-keys", "part-mod-desert", "part-mod-brains"]);
  // Installed: its page; not: install it here.
  expect(within(items[0]).getByRole("link", { name: /Details & Play/ })).toHaveAttribute(
    "href",
    "/profiles/keys-mod",
  );
  expect(within(items[1]).getByRole("button", { name: /Install/ })).toBeInTheDocument();
  // The page names the series once, not on every card.
  expect(within(episodes).queryByText(/Toast/)).not.toBeInTheDocument();

  const bonus = screen.getByRole("region", { name: "Bonus" });
  expect(within(bonus).getByRole("heading", { level: 3 })).toHaveTextContent("Desert Levelpack");
});

test("original games open their copy or say how to get them", async () => {
  renderPage("keen-4-6");
  const episodes = await screen.findByRole("region", { name: "Episodes" });
  const [keen4, keen5, keen6] = within(episodes).getAllByRole("listitem");
  expect(within(keen4).getByRole("heading")).toHaveTextContent("Secret of the Oracle");
  expect(within(keen4).getByRole("link", { name: /Details & Play/ })).toHaveAttribute(
    "href",
    "/profiles/keen4-a",
  );
  expect(within(keen5).getByText("Not in your library yet.")).toBeInTheDocument();
  expect(within(keen5).getByRole("link", { name: "Add game" })).toHaveAttribute("href", "/");
  expect(within(keen6).getByRole("heading")).toHaveTextContent("Commander Keen 6");
  expect(await within(keen6).findByRole("link", { name: "Download for free" })).toHaveAttribute(
    "href",
    "/discover?tab=games",
  );
  expect(screen.getByText("by id Software")).toBeInTheDocument();
});

test("an unknown series says so and leads to Discover", async () => {
  renderPage("nothing");
  expect(await screen.findByRole("heading", { name: "No such series" })).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Go to Discover" })).toHaveAttribute("href", "/discover");
});
