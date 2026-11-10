import { commands } from "../bindings";

/** Which controls a sheet shows. */
export type ControlsView = "keyboard" | "controller";

export const controllerConnectedKey = ["controllerConnected"] as const;

/** Longer than this, and the answer is taken as no. */
const patience = 300;

/**
 * Whether a controller is connected; no where that can't be told in time.
 * Only what BloogBase shows depends on it: the games get the controller
 * through DOSBox either way.
 */
export async function controllerConnected(): Promise<boolean> {
  const late = new Promise<boolean>((resolve) => setTimeout(() => resolve(false), patience));
  const asked = Promise.resolve()
    .then(() => commands.controllerConnected())
    .catch(() => false);
  return Promise.race([asked, late]);
}
