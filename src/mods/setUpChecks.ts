import type {
  Game,
  GameVersion,
  Install,
  KeenwikiOrigin,
  Mod,
  ModInfo,
  RecipeOption,
} from "../bindings";
import { bestFit, isModified } from "../catalog";

const same = (a: string, b: string) => a.trim().toLowerCase() === b.trim().toLowerCase();
// Known on both sides and different: another mod, whatever it's called.
const differ = (a: string | null | undefined, b: string | null | undefined) =>
  !!a && !!b && a !== b;

/**
 * Other versions of the mod the player has, to offer replacing: the one
 * picked to replace, same KeenWiki page, or same game and name (`name` as
 * typed or as detected).
 */
export function otherVersions(
  mods: Mod[],
  info: ModInfo,
  name: string,
  origin: KeenwikiOrigin | null,
  replacing: string | undefined,
): Mod[] {
  return mods.filter(
    (m) =>
      m.gameId === info.gameId &&
      (m.id === replacing ||
        (origin && m.keenwikiUrl === origin.pageUrl) ||
        ((same(m.name, name) || same(m.name, info.name)) &&
          !differ(m.keenwikiUrl, origin?.pageUrl) &&
          !differ(m.indexId, info.indexId))),
  );
}

/**
 * The copy to install on: the one the player picked, else the one of the
 * mod it replaces, else the best fit.
 */
export function pickTarget(
  candidates: Install[],
  picked: string | null,
  replacedInstall: string | null | undefined,
  game: Game | undefined,
): Install | undefined {
  return (
    candidates.find((i) => i.id === picked) ??
    candidates.find((i) => i.id === replacedInstall) ??
    bestFit(game, candidates)
  );
}

/** What the player should know before installing `option` on `target`. */
export type SetUpWarnings = {
  /** The KeenWiki page says another game than the files. */
  otherGame: boolean;
  /** No copy of the game it plays on. */
  missingGame: boolean;
  /** The version a patch naming none is likely for, when `target` is another. */
  assumedInstead: GameVersion | undefined;
  /** A patch on a third-party build. */
  modifiedGame: boolean;
  /** The patch names versions, and the target's is unknown. */
  versionUnchecked: boolean;
  /** Made to be copied over another mod, which isn't part of it. */
  incomplete: boolean;
  /** Levels alone, for the mod their archive names. */
  levelsFor: boolean;
  /** Levels alone, for a mod BloogBase doesn't know: its name. */
  unknownBase: string | null;
};

export function setUpWarnings({
  info,
  option,
  origin,
  game,
  candidates,
  target,
}: {
  info: ModInfo;
  option: RecipeOption;
  origin: KeenwikiOrigin | null;
  game: Game | undefined;
  /** Undefined while the library loads. */
  candidates: Install[] | undefined;
  target: Install | undefined;
}): SetUpWarnings {
  const versionOf = (i: Install) => game?.versions.find((v) => v.id === i.versionId);
  // A patch that names no version is most likely made for the version most mods use.
  const assumed =
    option.recipe.type === "patch" && option.gameVersions.length === 0
      ? game?.versions.find((v) => v.id === game.moddingVersion)
      : undefined;
  const targetVersion = target && versionOf(target);
  // Unknown versions get no warning: they may well be the right one.
  const assumedInstead =
    assumed && targetVersion && (targetVersion.original ?? targetVersion.id) !== assumed.id
      ? assumed
      : undefined;
  const incomplete = option.missingFiles.length > 0;
  return {
    otherGame: !!origin?.gameId && origin.gameId !== info.gameId,
    missingGame: candidates !== undefined && candidates.length === 0,
    assumedInstead,
    modifiedGame: !!target && option.recipe.type === "patch" && isModified(targetVersion),
    versionUnchecked: !!target && target.versionId === null && option.gameVersions.length > 0,
    incomplete,
    levelsFor: !!option.baseMod && !incomplete,
    unknownBase: !option.baseMod && !incomplete ? option.madeFor : null,
  };
}
