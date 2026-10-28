import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { KeenwikiCacheSection } from "./Settings";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({ Link: vi.fn(), useNavigate: vi.fn() }));
vi.mock("../bindings", () => ({ commands: { clearKeenwikiCache: vi.fn() } }));

test("deletes the saved KeenWiki pages", async () => {
  vi.mocked(commands.clearKeenwikiCache).mockResolvedValue({ status: "ok", data: null });
  render(
    <QueryClientProvider client={new QueryClient()}>
      <KeenwikiCacheSection />
    </QueryClientProvider>,
  );
  await userEvent.click(screen.getByRole("button", { name: "Delete saved pages" }));
  expect(commands.clearKeenwikiCache).toHaveBeenCalledOnce();
  expect(await screen.findByRole("status")).toHaveTextContent("Deleted.");
});
