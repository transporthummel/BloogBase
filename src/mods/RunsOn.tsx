import { Link } from "@tanstack/react-router";
import { Trans } from "react-i18next";
import type { Profile } from "../bindings";

/** "Runs on <game> <version>", for a mod played over another one "… and <that mod>", linked. */
export function RunsOn({
  profile,
  game,
  version,
}: {
  profile: Profile;
  game: string;
  version: string;
}) {
  const over = profile.playedOver;
  return (
    <Trans
      i18nKey="mods.baseGame"
      context={over ? "over" : undefined}
      values={{ game, version, mod: over?.name }}
      components={{
        mod: over ? (
          <Link
            to="/profiles/$profileId"
            params={{ profileId: over.profileId }}
            className="underline-offset-2 hover:text-accent hover:underline"
          />
        ) : (
          <span />
        ),
      }}
    />
  );
}
