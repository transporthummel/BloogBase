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
  hint,
  children,
}: {
  on: boolean;
  onClick: () => void;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onClick}
      title={hint}
      aria-description={hint}
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

/** A setting that's on or off, e.g. "Not installed". */
export function Toggle({
  on,
  onChange,
  hint,
  children,
}: {
  on: boolean;
  onChange: (on: boolean) => void;
  /** What it means, as a tooltip and description. */
  hint?: string;
  children: ReactNode;
}) {
  return (
    <Chip on={on} onClick={() => onChange(!on)} hint={hint}>
      {on && <Check className="-ml-0.5 size-3.5" aria-hidden />}
      {children}
    </Chip>
  );
}

/** `value` with `item` added, or taken out when it's in. */
function flip<T>(value: T[], item: T): T[] {
  return value.includes(item) ? value.filter((v) => v !== item) : [...value, item];
}

/** Any of a few choices, none for all of them. */
export function Choice<T extends string>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T[];
  onChange: (value: T[]) => void;
}) {
  const { t } = useTranslation();
  return (
    <div role="group" aria-label={label} className="flex flex-wrap gap-1.5">
      <Chip on={value.length === 0} onClick={() => onChange([])}>
        {t("discover.all")}
      </Chip>
      {options.map((o) => (
        <Chip
          key={o.value}
          on={value.includes(o.value)}
          onClick={() => onChange(flip(value, o.value))}
        >
          {o.label}
        </Chip>
      ))}
    </div>
  );
}

/**
 * The game filter as a row of the tiles on the cards: the episode number on
 * its engine's colour, "+" for mods that need no game. With some chosen, the
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
  value: string[];
  onChange: (value: string[]) => void;
}) {
  const { t } = useTranslation();
  const tiles = [
    ...ids.map((id) => ({ id, name: gameTitle(games, id) })),
    ...(noGame ? [{ id: NO_GAME, name: t("discover.noGame") }] : []),
  ];
  return (
    <div role="group" aria-label={t("discover.game")} className="flex flex-wrap gap-1.5">
      <Chip on={value.length === 0} onClick={() => onChange([])}>
        {t("discover.all")}
      </Chip>
      {tiles.map(({ id, name }) => (
        <button
          key={id}
          type="button"
          aria-pressed={value.includes(id)}
          aria-label={name}
          title={name}
          onClick={() => onChange(flip(value, id))}
          className={`rounded-md transition active:translate-y-px active:duration-0 ${
            value.includes(id)
              ? "ring-2 ring-accent ring-offset-2 ring-offset-bg"
              : value.length
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
