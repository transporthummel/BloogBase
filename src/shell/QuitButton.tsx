import { Power } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { navItemClass } from "./navItemClass";

/**
 * Closes BloogBase as its window's close button does, so it asks first
 * while games run. Needed in fullscreen, which has no title bar.
 */
export function QuitButton() {
  const { t } = useTranslation();
  const label = t("nav.quit");
  return (
    <button
      type="button"
      title={label}
      onClick={() => void commands.closeApp()}
      className={navItemClass}
    >
      <Power className="size-5 shrink-0" aria-hidden />
      <span className="sr-only xl:not-sr-only xl:text-sm xl:font-medium">{label}</span>
    </button>
  );
}
