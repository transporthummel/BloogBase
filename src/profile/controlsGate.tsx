import { useState } from "react";
import type { Game, Profile } from "../bindings";
import { getControlsShown, setControlsShown } from "../preferences";
import { controllerConnected, type ControlsView } from "./controller";
import { ControlsDialog } from "./Controls";

/**
 * Starts a game or mod, first showing the controls if its series hasn't
 * been played with BloogBase on this device (or the player asked to see
 * them again). With a controller connected, that's the controller's layout,
 * remembered apart from the keys. Render `dialog` somewhere on the page.
 */
export function useControlsGate(start: (profileId: string) => void) {
  const [pending, setPending] = useState<{
    profile: Profile;
    game: Game;
    view: ControlsView;
  } | null>(null);
  async function play(profile: Profile, game: Game | undefined) {
    if (!game) return start(profile.id);
    const view: ControlsView = (await controllerConnected()) ? "controller" : "keyboard";
    if (getControlsShown(view).includes(game.engine)) start(profile.id);
    else setPending({ profile, game, view });
  }
  const dialog = pending && (
    <ControlsDialog
      profile={pending.profile}
      game={pending.game}
      view={pending.view}
      onClose={() => setPending(null)}
      onStart={(remember) => {
        if (remember) setControlsShown(pending.game.engine, pending.view);
        setPending(null);
        start(pending.profile.id);
      }}
    />
  );
  return { play, dialog };
}
