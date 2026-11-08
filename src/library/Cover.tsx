import type { Game } from "../bindings";

const engineColor = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
} as const;

/**
 * The game's title screen when `src` is given, else a placeholder: the
 * episode number on the engine's color, or for a source mod, which shares
 * nothing with that episode but its engine, a "+" on dark cyan.
 */
export function Cover({
  gameId,
  game,
  src,
  standalone = false,
  className = "aspect-[4/3] text-5xl",
}: {
  gameId: string;
  game?: Game;
  src?: string | null;
  standalone?: boolean;
  className?: string;
}) {
  if (src) {
    // 320x200 pixels shown at 4:3, as on a CRT.
    return (
      <img
        src={src}
        alt=""
        loading="lazy"
        decoding="async"
        className={`object-fill [image-rendering:pixelated] ${className}`}
      />
    );
  }
  if (standalone) {
    return (
      <div
        className={`flex items-center justify-center bg-source font-pixel text-white ${className}`}
        aria-hidden
      >
        +
      </div>
    );
  }
  const episode = gameId.replace(/^keen/, "").replace(/^kdreams$/, "D");
  return (
    <div
      className={`flex items-center justify-center font-pixel ${
        game ? `${engineColor[game.engine]} text-white` : "bg-surface-raised text-muted"
      } ${className}`}
      aria-hidden
    >
      {episode}
    </div>
  );
}
