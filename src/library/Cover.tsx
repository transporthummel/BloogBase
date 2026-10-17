import type { Game } from "../bindings";

const engineColor = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
} as const;

/** Placeholder cover: the episode number on the engine's color. */
export function Cover({
  gameId,
  game,
  className = "aspect-[3/4] text-5xl",
}: {
  gameId: string;
  game?: Game;
  className?: string;
}) {
  const episode = gameId.replace(/^keen/, "").replace(/^kdreams$/, "D");
  return (
    <div
      className={`flex items-center justify-center font-pixel text-white/90 ${
        game ? engineColor[game.engine] : "bg-surface-raised"
      } ${className}`}
      aria-hidden
    >
      {episode}
    </div>
  );
}
