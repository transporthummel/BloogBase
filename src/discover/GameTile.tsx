import type { Game } from "../bindings";
import { Cover } from "../library/Cover";

/**
 * The game a Discover card is for: the episode number on its engine's
 * colour, as a small tile beside the title. Nothing is installed to draw a
 * title screen from, so it stays a marker rather than a picture.
 */
export function GameTile({
  gameId,
  game,
  standalone = false,
}: {
  gameId: string;
  game?: Game;
  /** A source mod: not the episode's number, which it shares nothing but its engine with. */
  standalone?: boolean;
}) {
  return (
    <Cover
      gameId={gameId}
      game={game}
      standalone={standalone}
      className="size-11 shrink-0 rounded-md text-base"
    />
  );
}
