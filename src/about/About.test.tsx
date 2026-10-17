import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { About } from "./About";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: { appInfo: vi.fn(), runtimeStatus: vi.fn() },
}));

test("credits the author with a link to the PCKF profile", async () => {
  vi.mocked(commands.appInfo).mockResolvedValue({
    status: "ok",
    data: { version: "0.1.0", dataDir: "/d", defaultDataDir: "/d", schemaVersion: 4 },
  });
  vi.mocked(commands.runtimeStatus).mockResolvedValue({
    status: "ok",
    data: { active: null, managed: null, pinnedVersion: "0.83.0", downloadAvailable: true },
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <About />
    </QueryClientProvider>,
  );
  expect(screen.getByText(/Made by/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "transporthummel" }));
  expect(openUrl).toHaveBeenCalledWith("https://pckf.com/memberlist.php?mode=viewprofile&u=16709");
});
