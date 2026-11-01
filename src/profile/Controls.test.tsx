import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vitest";
import { commands, type Firing, type Game, type Profile } from "../bindings";
import { ControlsSheet } from "./Controls";

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: React.ReactNode }) => <a>{children}</a>,
}));
vi.mock("../bindings", () => ({
  commands: { oneButtonFiring: vi.fn(), profileFiring: vi.fn() },
}));

const ok = <T,>(data: T) => ({ status: "ok" as const, data });

function game(id: string, engine: Game["engine"]): Game {
  return {
    id,
    title: id,
    shortTitle: id,
    engine,
    executables: [],
    requiredFiles: [],
    saves: [],
    stateFiles: [],
    versions: [],
  };
}

const profile = (modId: string | null = null) =>
  ({ id: "p", modId, gameId: "keen1", label: "Keen" }) as Profile;

function renderSheet(g: Game, p = profile()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ControlsSheet profile={p} game={g} />
    </QueryClientProvider>,
  );
}

function setFiring(enabled: boolean, firing: Firing | null) {
  vi.mocked(commands.oneButtonFiring).mockResolvedValue(ok(enabled));
  vi.mocked(commands.profileFiring).mockResolvedValue(ok(firing));
}

/** The keys shown for an action, e.g. "Ctrl+Alt". */
function keysFor(action: string) {
  const row = screen.getByText(action).closest("li")!;
  return [...row.querySelectorAll("kbd")].map((k) => k.textContent).join("+");
}

beforeEach(() => setFiring(false, null));

test("Keen 1-3 shoot with Ctrl and Alt, Space opens the status window", async () => {
  setFiring(false, "patchable");
  renderSheet(game("keen1", "vorticons"));
  expect(keysFor("Pogo on and off, once you've found it")).toBe("Alt");
  expect(keysFor("Shoot")).toBe("Ctrl+Alt");
  expect(keysFor("Status window")).toBe("Space");
  // The patch fits but is off: a pointer to the setting.
  expect(await screen.findByText("Turn it on in Settings.")).toBeInTheDocument();
});

test("with one-button firing on, Space shoots and Enter opens the status window", async () => {
  setFiring(true, "patchable");
  renderSheet(game("keen2", "vorticons"));
  await vi.waitFor(() => expect(keysFor("Shoot")).toBe("Space"));
  expect(keysFor("Status window")).toBe("Enter");
  expect(keysFor("Pogo on and off")).toBe("Alt");
  expect(screen.queryByText("Turn it on in Settings.")).not.toBeInTheDocument();
});

test("a mod that shoots with Space shows it with the setting off", async () => {
  setFiring(false, "builtIn");
  renderSheet(game("keen3", "vorticons"), profile("mod"));
  await vi.waitFor(() => expect(keysFor("Shoot")).toBe("Space"));
  expect(screen.getByText(/Mods may change keys and cheats/)).toBeInTheDocument();
});

test("cheats are folded away, and Keen 4-6 list the debug mode's", async () => {
  renderSheet(game("keen4", "galaxy"));
  expect(keysFor("Shoot")).toBe("Space");
  expect(keysFor("Status window")).toBe("Enter");
  const cheats = screen.getByText("Cheat codes").closest("details")!;
  expect(cheats).not.toHaveAttribute("open");
  await userEvent.click(screen.getByText("Cheat codes"));
  expect(cheats).toHaveAttribute("open");
  expect(keysFor("All gems, 99 shots and an extra life")).toBe("B+A+T");
  expect(within(cheats).getByText(/First turn on debug mode with/)).toBeInTheDocument();
  expect(keysFor("God mode: nothing can hurt Keen")).toBe("F10+G");
});

test("Keen Dreams throws Flower Power and has F10 cheats without a debug mode", () => {
  renderSheet(game("kdreams", "dreams"));
  expect(keysFor("Throw Flower Power")).toBe("Alt");
  expect(keysFor("Status window")).toBe("Space");
  expect(screen.queryByText("Shoot")).not.toBeInTheDocument();
  expect(screen.getByText("Hold F10 and press:")).toBeInTheDocument();
  expect(commands.profileFiring).not.toHaveBeenCalled();
});
