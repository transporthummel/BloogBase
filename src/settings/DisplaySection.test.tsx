import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { DisplaySection } from "./Settings";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({ Link: vi.fn(), useNavigate: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: { displayOptions: vi.fn(), setDisplayOptions: vi.fn() },
}));

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const defaults = { fullscreen: false, crt: true, vsync: false };

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.setDisplayOptions).mockResolvedValue(ok(null));
});

function renderSection() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <DisplaySection />
    </QueryClientProvider>,
  );
}

test("toggling an option saves all options", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(ok({ options: defaults, overridden: [] }));
  renderSection();
  const fullscreen = await screen.findByRole("checkbox", { name: "Start games in fullscreen" });
  expect(fullscreen).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "CRT monitor effect" })).toBeChecked();
  await userEvent.click(fullscreen);
  expect(commands.setDisplayOptions).toHaveBeenCalledWith({ ...defaults, fullscreen: true });
});

test("options the DOSBox settings set are locked and say so", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: defaults, overridden: ["vsync"] }),
  );
  renderSection();
  expect(
    await screen.findByRole("checkbox", { name: "Prevent tearing while scrolling (VSync)" }),
  ).toBeDisabled();
  expect(
    screen.getByText("Set by your DOSBox settings for all games (below)."),
  ).toBeInTheDocument();
  expect(screen.getByRole("checkbox", { name: "CRT monitor effect" })).toBeEnabled();
});
