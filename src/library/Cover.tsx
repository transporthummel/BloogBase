import type { Game } from "../bindings";

const engineColor = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
} as const;

/**
 * The game's title screen when `src` is given, else a placeholder: the
 * episode number on the engine's color.
 */
export function Cover({
  gameId,
  game,
  src,
  className = "aspect-[4/3] text-5xl",
}: {
  gameId: string;
  game?: Game;
  src?: string | null;
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
