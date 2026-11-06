import { useTranslation } from "react-i18next";

const arrows: Record<string, string> = { left: "←", right: "→", up: "↑", down: "↓" };
// Keys whose name is a word; the rest (letters, digits, F-keys) show as they are printed.
const named = new Set([
  "ctrl",
  "alt",
  "space",
  "enter",
  "esc",
  "tab",
  "shiftLeft",
  "shiftRight",
  "cmd",
  "backspace",
  "capsLock",
  "numLock",
  "scrollLock",
  "home",
  "end",
  "pageUp",
  "pageDown",
  "insert",
  "delete",
]);

/** One key, drawn as a keycap. */
export function Keycap({
  id,
  changed = false,
  unused = false,
}: {
  id: string;
  changed?: boolean;
  /** Drawn faintly: a key of the cluster the game doesn't use. */
  unused?: boolean;
}) {
  const { t } = useTranslation();
  const arrow = arrows[id];
  const label = arrow ?? (named.has(id) ? t(`controls.key.${id}`) : id.toUpperCase());
  return (
    <kbd
      aria-label={arrow ? t(`controls.key.${id}`) : undefined}
      className={`inline-flex h-8 shrink-0 items-center justify-center rounded-md border border-b-[3px] bg-surface-raised px-2 font-sans text-xs font-semibold shadow-key transition-[translate,border-width] duration-150 hover:translate-y-px hover:border-b-2 ${
        id === "space" ? "min-w-24" : "min-w-8"
      } ${changed ? "border-accent/70 text-accent" : "border-border text-ink"} ${unused ? "opacity-40" : ""}`}
    >
      {label}
    </kbd>
  );
}
