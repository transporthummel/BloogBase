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
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  /** The arrow's accessible name. */
  menuLabel: string;
  items: MenuItem[];
  variant?: Variant;
}) {
  return (
    <div className="flex items-stretch">
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
        className="rounded-l-none border-l border-accent-ink/25 px-2.5"
        trigger={<ChevronDown className="size-4" aria-hidden />}
        items={items}
      />
    </div>
  );
}
