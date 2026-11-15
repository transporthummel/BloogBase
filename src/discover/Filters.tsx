import { Check } from "lucide-react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { gameTitle } from "../catalog";
import type { Game } from "../bindings";
import { GameTile } from "./GameTile";
import { NO_GAME } from "./fit";

/** A pill that's on or off; on, it takes the green tint of the player's own state. */
function Chip({
  on,
  onClick,
  children,
}: {
  on: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      className={`inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-sm font-semibold transition active:translate-y-px active:duration-0 ${
        on
          ? "border-accent/50 bg-accent/10 text-accent"
          : "border-border text-muted hover:border-muted hover:text-ink"
      }`}
    >
      {children}
    </button>
  );
}

/** A setting that's on or off, e.g. "Playable". */
export function Toggle({
  on,
  onChange,
  children,
}: {
  on: boolean;
  onChange: (on: boolean) => void;
  children: ReactNode;
}) {
  return (
    <Chip on={on} onClick={() => onChange(!on)}>
      {on && <Check className="-ml-0.5 size-3.5" aria-hidden />}
      {children}
    </Chip>
  );
}

/** One of a few choices, "" for all of them. */
export function Choice<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T | "";
  onChange: (value: T | "") => void;
}) {
  const { t } = useTranslation();
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1.5">
      <Chip on={value === ""} onClick={() => onChange("")}>
        {t("discover.all")}
      </Chip>
      {options.map((o) => (
        <Chip key={o.value} on={o.value === value} onClick={() => onChange(o.value)}>
          {o.label}
        </Chip>
      ))}
    </div>
  );
}

/**
 * The game filter as a row of the tiles on the cards: the episode number on
 * its engine's colour, "+" for mods that need no game. With one chosen, the
 * others fade.
 */
export function GameChoice({
  games,
  ids,
  noGame,
  value,
  onChange,
}: {
  games: Game[];
  /** The games with mods, in catalog order. */
  ids: string[];
  /** Whether there are mods that need no game. */
  noGame: boolean;
  value: string;
  onChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  const tiles = [
    ...ids.map((id) => ({ id, name: gameTitle(games, id) })),
    ...(noGame ? [{ id: NO_GAME, name: t("discover.noGame") }] : []),
  ];
  return (
    <div role="group" aria-label={t("discover.game")} className="flex flex-wrap gap-1.5">
      <Chip on={value === ""} onClick={() => onChange("")}>
        {t("discover.all")}
      </Chip>
      {tiles.map(({ id, name }) => (
        <button
          key={id}
          type="button"
          aria-pressed={id === value}
          aria-label={name}
          title={name}
          onClick={() => onChange(id === value ? "" : id)}
          className={`rounded-md transition active:translate-y-px active:duration-0 ${
            id === value
              ? "ring-2 ring-accent ring-offset-2 ring-offset-bg"
              : value
                ? "opacity-40 hover:opacity-100"
                : ""
          }`}
        >
          <GameTile
            gameId={id}
            game={games.find((g) => g.id === id)}
            standalone={id === NO_GAME}
            className="size-9 text-sm"
          />
        </button>
      ))}
    </div>
  );
}
