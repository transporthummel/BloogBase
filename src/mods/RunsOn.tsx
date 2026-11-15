import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { Trans } from "react-i18next";
import type { Profile } from "../bindings";

const linkClass = "underline-offset-2 hover:text-accent hover:underline";

/** A link to a profile's page, or plain text without one. `Trans` fills in the text. */
function ProfileLink({
  profileId,
  children,
}: {
  profileId: string | null | undefined;
  children?: ReactNode;
}) {
  return profileId ? (
    <Link to="/profiles/$profileId" params={{ profileId }} className={linkClass}>
      {children}
    </Link>
  ) : (
    <span>{children}</span>
  );
}

/**
 * "Runs on <game> <version>", for a mod played over another one "… and <that
 * mod>", both linked. A mod that needs no copy of its game: "<game> ·
 * Standalone", "<game> · Runs on <that mod>" over one (Lego Levels over Lego
 * Keen).
 */
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
      context={
        profile.standalone ? (over ? "standaloneOver" : "standalone") : over ? "over" : undefined
      }
      values={{ game, version, mod: over?.name }}
      components={{
        game: <ProfileLink profileId={profile.gameProfileId} />,
        mod: <ProfileLink profileId={over?.profileId} />,
      }}
    />
  );
}
