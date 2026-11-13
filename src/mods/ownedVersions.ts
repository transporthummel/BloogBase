import type { Game, Install } from "../bindings";
import { useInstalls } from "../library/queries";

/**
 * The versions of the player's copies of `gameId`, e.g. `v1.4 EGA
 * (FormGen)`, or `null` without one. Where a mod fits none of them, they
 * are the wrong versions.
 */
export function ownedVersions(installs: Install[], gameId: string, game: Game | undefined) {
  const labels = installs
    .filter((i) => i.gameId === gameId)
    .map((i) => game?.versions.find((v) => v.id === i.versionId)?.label)
    .filter((l): l is string => l !== undefined);
  return labels.length === 0 ? null : [...new Set(labels)].join(", ");
}

/** {@link ownedVersions} from the library. */
export function useOwnedVersions(gameId: string, game: Game | undefined) {
  const installs = useInstalls();
  return ownedVersions(installs.data ?? [], gameId, game);
}
