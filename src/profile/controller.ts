import { useQuery } from "@tanstack/react-query";
import { commands, type Controller } from "../bindings";
import { padLayout } from "../gamepad/buttons";
import { unwrap } from "../ipc";

/** Which controls a sheet shows. */
export type ControlsView = "keyboard" | "controller";

export const controllerConnectedKey = ["controllerConnected"] as const;
/** The layout chosen in the settings; null: found by itself. */
export const controllerLayoutKey = ["controllerLayout"] as const;

/** Longer than this, and the answer is taken as no. */
const patience = 300;

/**
 * Which controller is connected; none where that can't be told in time.
 * Only what BloogBase shows depends on it: the games get the controller
 * through DOSBox either way.
 */
export async function connectedController(): Promise<Controller | null> {
  const late = new Promise<null>((resolve) => setTimeout(() => resolve(null), patience));
  const asked = Promise.resolve()
    .then(() => commands.controllerConnected())
    .catch(() => null);
  return Promise.race([asked, late]);
}

/** Whether a controller is connected; no where that can't be told in time. */
export async function controllerConnected(): Promise<boolean> {
  return (await connectedController()) !== null;
}

/**
 * The controller layout whose button names to show: the one chosen in the
 * settings, else the connected one, else that of the pad the app sees (as
 * on the Controls tab).
 */
export function usePadLayout(): Controller {
  const chosen = useQuery({
    queryKey: controllerLayoutKey,
    queryFn: () => unwrap(commands.controllerLayout()),
  });
  const connected = useQuery({
    queryKey: controllerConnectedKey,
    queryFn: connectedController,
    staleTime: 0,
  });
  return chosen.data ?? connected.data ?? padLayout();
}
