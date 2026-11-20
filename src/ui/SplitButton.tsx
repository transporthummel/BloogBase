import { ChevronDown } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";
import type { Variant } from "./buttonClass";
import { Menu, type MenuItem } from "./Menu";
import { PlayButton } from "./PlayButton";

/**
 * Play with more ways to start beside it: the main action, which says what
 * it does, and an arrow opening a menu of the others. Both halves are
 * buttons of their own, so keyboard and controller reach each.
 */
export function SplitButton({
  menuLabel,
  items,
  variant = "primary",
  className = "",
  wrapperClassName = "",
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  /** The arrow's accessible name. */
  menuLabel: string;
  items: MenuItem[];
  variant?: Variant;
  /** Sizes both halves together, e.g. `flex-1` on a card. */
  wrapperClassName?: string;
}) {
  return (
    <div className={`flex items-stretch ${wrapperClassName}`}>
      <PlayButton
        variant={variant}
        disabled={disabled}
        className={`rounded-r-none ${className}`}
        {...props}
      />
      <Menu
        label={menuLabel}
        variant={variant}
        disabled={disabled}
        // Its drop shadow would fall left onto the main half, a dark smudge
        // in the dark theme: cut off there, except while the focus ring
        // needs that side.
        // A card's outlined halves share the main half's right border, and
        // its arrow is narrower, to fit beside the label on a narrow card.
        className={`rounded-l-none ${variant === "card" ? "border-l-0 px-2!" : "border-l border-accent-ink/25 px-2.5"} [clip-path:inset(-2rem_-2rem_-2rem_0)] focus-visible:[clip-path:none] pad-focus:[clip-path:none]`}
        trigger={<ChevronDown className="size-4" aria-hidden />}
        items={items}
      />
    </div>
  );
}
