import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { Update } from "@tauri-apps/plugin-updater";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Profile } from "../bindings";
import { UpdateCard } from "./UpdateCard";

vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: { listProfiles: vi.fn(), restartApp: vi.fn() },
}));

const profile = { id: "keen4-1", running: false } as Profile;

function update() {
  return {
    version: "1.1.0",
    body: "Screenshots everywhere.",
    downloadAndInstall: vi.fn(async (cb: (e: unknown) => void) => {
      cb({ event: "Started", data: { contentLength: 10 } });
      cb({ event: "Progress", data: { chunkLength: 10 } });
      cb({ event: "Finished" });
    }),
  } as unknown as Update;
}

function renderCard(u: Update) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <UpdateCard update={u} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.listProfiles).mockResolvedValue({ status: "ok", data: [profile] });
});

test("installs the update and restarts", async () => {
  const u = update();
  renderCard(u);
  expect(screen.getByText("BloogBase 1.1.0 is available")).toBeInTheDocument();
  expect(screen.getByText("Screenshots everywhere.")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Install and restart" }));
  expect(u.downloadAndInstall).toHaveBeenCalled();
  await vi.waitFor(() => expect(commands.restartApp).toHaveBeenCalled());
});

test("waits while a game runs", async () => {
  vi.mocked(commands.listProfiles).mockResolvedValue({
    status: "ok",
    data: [{ ...profile, running: true }],
  });
  renderCard(update());
  expect(await screen.findByText(/Close the running game first/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Install and restart" })).toBeDisabled();
});
