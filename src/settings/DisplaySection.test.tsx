import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands } from "../bindings";
import { DisplaySection, InactiveSection, LegacyPcSection } from "./Settings";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("@tanstack/react-router", () => ({ Link: vi.fn(), useNavigate: vi.fn() }));
vi.mock("../bindings", () => ({
  commands: {
    displayOptions: vi.fn(),
    setDisplayOptions: vi.fn(),
    startFullscreen: vi.fn(),
    setStartFullscreen: vi.fn(),
  },
}));

const ok = <T,>(data: T) => ({ status: "ok" as const, data });
const defaults = {
  fullscreen: false,
  crt: true,
  vsync: false,
  inactive: "run" as const,
  legacyPc: false,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(commands.setDisplayOptions).mockResolvedValue(ok(null));
  vi.mocked(commands.startFullscreen).mockResolvedValue(ok(false));
  vi.mocked(commands.setStartFullscreen).mockResolvedValue(ok(null));
});

function renderSection(section = <DisplaySection />) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}>{section}</QueryClientProvider>);
}

test("toggling an option saves all options", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(ok({ options: defaults, overridden: [] }));
  renderSection();
  const vsync = await screen.findByRole("checkbox", {
    name: "Prevent tearing while scrolling (VSync)",
  });
  expect(vsync).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "CRT monitor effect" })).toBeChecked();
  await userEvent.click(vsync);
  expect(commands.setDisplayOptions).toHaveBeenCalledWith({ ...defaults, vsync: true });
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

test("choosing what a game does in the background saves all options", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(ok({ options: defaults, overridden: [] }));
  renderSection(<InactiveSection />);
  const select = await screen.findByRole("combobox", { name: "When you switch to another window" });
  expect(select).toHaveValue("run");
  await userEvent.selectOptions(select, "pause");
  expect(commands.setDisplayOptions).toHaveBeenCalledWith({ ...defaults, inactive: "pause" });
});

test("the background choice is locked when the DOSBox settings set it", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: defaults, overridden: ["inactive"] }),
  );
  renderSection(<InactiveSection />);
  expect(
    await screen.findByRole("combobox", { name: "When you switch to another window" }),
  ).toBeDisabled();
  expect(
    screen.getByText("Set by your DOSBox settings for all games (below)."),
  ).toBeInTheDocument();
});

test("the legacy PC mode is off by default and saves all options", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(ok({ options: defaults, overridden: [] }));
  renderSection(<LegacyPcSection />);
  const box = await screen.findByRole("checkbox", { name: "Hard disk noises and slower loading" });
  expect(box).not.toBeChecked();
  await userEvent.click(box);
  expect(commands.setDisplayOptions).toHaveBeenCalledWith({ ...defaults, legacyPc: true });
});

test("the legacy PC mode is locked when the DOSBox settings set it", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: defaults, overridden: ["legacyPc"] }),
  );
  renderSection(<LegacyPcSection />);
  expect(
    await screen.findByRole("checkbox", { name: "Hard disk noises and slower loading" }),
  ).toBeDisabled();
});

test("fullscreen shows both settings as one choice", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: { ...defaults, fullscreen: true }, overridden: [] }),
  );
  renderSection();
  const select = await screen.findByRole("combobox", { name: "Start in fullscreen" });
  expect(select).toHaveValue("games");
});

test("choosing BloogBase and games sets both", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(ok({ options: defaults, overridden: [] }));
  renderSection();
  const select = await screen.findByRole("combobox", { name: "Start in fullscreen" });
  expect(select).toHaveValue("none");
  await userEvent.selectOptions(select, "BloogBase and games");
  expect(commands.setDisplayOptions).toHaveBeenCalledWith({ ...defaults, fullscreen: true });
  expect(commands.setStartFullscreen).toHaveBeenCalledWith(true);
});

test("choosing nothing turns off BloogBase's fullscreen and the games'", async () => {
  vi.mocked(commands.startFullscreen).mockResolvedValue(ok(true));
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: { ...defaults, fullscreen: true }, overridden: [] }),
  );
  renderSection();
  const select = await screen.findByRole("combobox", { name: "Start in fullscreen" });
  expect(select).toHaveValue("both");
  await userEvent.selectOptions(select, "Nothing");
  expect(commands.setDisplayOptions).toHaveBeenCalledWith(defaults);
  expect(commands.setStartFullscreen).toHaveBeenCalledWith(false);
});

test("fullscreen set by the DOSBox settings leaves the games' option alone", async () => {
  vi.mocked(commands.displayOptions).mockResolvedValue(
    ok({ options: defaults, overridden: ["fullscreen"] }),
  );
  renderSection();
  const select = await screen.findByRole("combobox", { name: "Start in fullscreen" });
  await userEvent.selectOptions(select, "BloogBase and games");
  expect(commands.setDisplayOptions).not.toHaveBeenCalled();
  expect(commands.setStartFullscreen).toHaveBeenCalledWith(true);
});
