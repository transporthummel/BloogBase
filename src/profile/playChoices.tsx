import { Play } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { Profile, WorldStart } from "../bindings";
import type { MenuItem } from "../ui/Menu";
import { NewWorldDialog } from "./NewWorldDialog";
import { useVariants, variantName } from "./variants";
import { asksFirst, useWorlds, worldName } from "./worlds";

/** How many other worlds the arrow beside Continue offers. */
const RECENT_WORLDS = 5;

/**
 * What Play does for a mod with variants or worlds, and the other ways to
 * start it for the arrow beside it (§9.6, §9.7); the page's and the library
 * card's. `split` is null where there is nothing to offer beside Play.
 * `short` names Continue without "world", for the narrow library card.
 * Render `dialog` somewhere on the page.
 */
export function usePlayChoices(
  profile: Profile,
  /** With the world to play, for a mod that keeps worlds, or a variant. */
  onPlay: (world: WorldStart | null, variant?: string) => void,
  { short = false }: { short?: boolean } = {},
) {
  const { t, i18n } = useTranslation();
  const [asking, setAsking] = useState(false);
  const worlds = useWorlds(profile);
  const variants = useVariants(profile);
  // Once chosen, Play starts the variant played; the arrow the others.
  const otherVariants: MenuItem[] = profile.variantChosen
    ? (variants.data?.variants ?? [])
        .filter((v) => !v.active)
        .map((v) => ({
          // Just the name: the arrow is "Other variants", the icon says it plays.
          label: variantName(v.variant, i18n.language),
          icon: <Play className="size-4" />,
          onSelect: () => onPlay(null, v.variant.id),
        }))
    : [];
  // A new world asks for its seed and options first, where the mod has any.
  const asks = !!worlds.data && asksFirst(worlds.data.definition);
  const newWorld = () => {
    if (asks) setAsking(true);
    else onPlay({ type: "new", seed: null, options: {} });
  };
  const newLabel = asks ? t("worlds.newEllipsis") : t("worlds.new");
  // Up to five worlds but the one Continue starts; beside Continue, a new
  // one too. Without a world going on, a new one is the main action.
  const active = profile.activeWorld;
  const recent: MenuItem[] = (worlds.data?.worlds ?? [])
    .filter((w) => !w.active)
    // Favourites first, each group by last start.
    .sort((a, b) => Number(b.favourite) - Number(a.favourite) || b.lastPlayedAt - a.lastPlayedAt)
    .slice(0, RECENT_WORLDS)
    .map((w, i) => ({
      label: worldName(t, w),
      icon: <Play className="size-4" />,
      onSelect: () => onPlay({ type: "continue", id: w.id }),
      separated: active !== null && i === 0,
    }));
  const startItems: MenuItem[] =
    active !== null ? [{ label: t("worlds.newEllipsis"), onSelect: newWorld }, ...recent] : recent;

  const split =
    otherVariants.length > 0
      ? {
          kind: "variants" as const,
          label: t("library.play"),
          play: () => onPlay(null),
          items: otherVariants,
        }
      : profile.worlds && startItems.length > 0
        ? {
            kind: "worlds" as const,
            // Says what it does: the world played last goes on, else a new
            // one begins.
            label:
              active !== null ? t(short ? "worlds.continueShort" : "worlds.continue") : newLabel,
            play: () => (active !== null ? onPlay({ type: "continue", id: active }) : newWorld()),
            items: startItems,
          }
        : null;

  const dialog = asking && worlds.data && (
    <NewWorldDialog
      definition={worlds.data.definition}
      lastOptions={worlds.data.lastOptions}
      worlds={worlds.data.worlds}
      onClose={() => setAsking(false)}
      onStart={(start) => {
        setAsking(false);
        onPlay(start);
      }}
    />
  );
  return { split, newWorld, newLabel, dialog };
}
