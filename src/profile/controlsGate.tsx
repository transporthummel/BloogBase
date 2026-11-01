import { useState } from "react";
import type { Game, Profile } from "../bindings";
import { getControlsShown, setControlsShown } from "../preferences";
import { ControlsDialog } from "./Controls";

/**
 * Starts a game or mod, first showing the controls if its series hasn't
 * been played with BloogBase on this device (or the player asked to see
 * them again). Render `dialog` somewhere on the page.
 */
export function useControlsGate(start: (profileId: string) => void) {
  const [pending, setPending] = useState<{ profile: Profile; game: Game } | null>(null);
  function play(profile: Profile, game: Game | undefined) {
    if (game && !getControlsShown().includes(game.engine)) setPending({ profile, game });
    else start(profile.id);
  }
  const dialog = pending && (
    <ControlsDialog
      profile={pending.profile}
      game={pending.game}
      onClose={() => setPending(null)}
      onStart={(remember) => {
        if (remember) setControlsShown(pending.game.engine);
        setPending(null);
        start(pending.profile.id);
      }}
    />
  );
  return { play, dialog };
}
