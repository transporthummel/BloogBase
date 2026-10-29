import type { Game } from "../bindings";
import { Cover } from "../library/Cover";

/**
 * The game a Discover card is for: the episode number on its engine's
 * colour, as a small tile beside the title. Nothing is installed to draw a
 * title screen from, so it stays a marker rather than a picture.
 */
export function GameTile({ gameId, game }: { gameId: string; game?: Game }) {
  return <Cover gameId={gameId} game={game} className="size-11 shrink-0 rounded-md text-base" />;
}
