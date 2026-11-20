import { useState } from "react";
import type { Profile } from "../bindings";
import { VariantDialog } from "./VariantDialog";

/**
 * Starts a mod with variants, first asking which one while the player
 * hasn't chosen (the first start, also after a mod got its variants): with
 * the one played so far, else the default, preselected. `variant` given
 * starts that one. Render `dialog` somewhere on the page.
 */
export function useVariantGate(go: (profile: Profile, variant: string | null) => void) {
  const [asking, setAsking] = useState<Profile | null>(null);
  function play(profile: Profile, variant?: string | null) {
    if (variant === undefined && profile.variantCount > 0 && !profile.variantChosen)
      setAsking(profile);
    else go(profile, variant ?? null);
  }
  const dialog = asking && (
    <VariantDialog
      profile={asking}
      onClose={() => setAsking(null)}
      onPlay={(variant) => {
        setAsking(null);
        go(asking, variant);
      }}
    />
  );
  return { play, dialog };
}
