import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Maximize, Minimize } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { navItemClass } from "./navItemClass";

const windowFullscreenKey = ["windowFullscreen"] as const;

/**
 * Puts BloogBase's window in or out of fullscreen, from the sidebar or with
 * F11. While it is in fullscreen, games start in fullscreen too.
 */
export function FullscreenButton() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const fullscreen = useQuery({
    queryKey: windowFullscreenKey,
    queryFn: () => commands.windowFullscreen(),
  });
  const toggle = useMutation({
    mutationFn: (on: boolean) => unwrap(commands.setWindowFullscreen(on)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: windowFullscreenKey }),
  });
  const on = fullscreen.data ?? false;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "F11" || e.repeat || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
      e.preventDefault();
      if (!toggle.isPending) toggle.mutate(!on);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [on, toggle]);

  const label = on ? t("nav.fullscreenLeave") : t("nav.fullscreen");
  const Icon = on ? Minimize : Maximize;
  return (
    <button
      type="button"
      title={`${label} (F11)`}
      disabled={fullscreen.data === undefined}
      onClick={() => toggle.mutate(!on)}
      className={navItemClass}
    >
      <Icon className="size-5 shrink-0" aria-hidden />
      <span className="sr-only xl:not-sr-only xl:text-sm xl:font-medium">{label}</span>
    </button>
  );
}
