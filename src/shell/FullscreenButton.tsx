import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Maximize, Minimize } from "lucide-react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { isMac } from "../platform";
import { navItemClass } from "./navItemClass";

const windowFullscreenKey = ["windowFullscreen"] as const;

/**
 * Puts BloogBase's window in or out of fullscreen, from the sidebar or with
 * F11. While it is in fullscreen, games start in fullscreen too. On macOS,
 * where F11 shows the desktop, the shortcut named is ⌃⌘F from Tauri's
 * default menu (View), which like the window's green button switches
 * without us: the button follows the window's size.
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

  useEffect(() => {
    if (!isMac()) return;
    const unlisten = getCurrentWindow().onResized(() => {
      void queryClient.invalidateQueries({ queryKey: windowFullscreenKey });
    });
    return () => {
      void unlisten.then((f) => f());
    };
  }, [queryClient]);

  const label = on ? t("nav.fullscreenLeave") : t("nav.fullscreen");
  const Icon = on ? Minimize : Maximize;
  return (
    <button
      type="button"
      title={`${label} (${isMac() ? "⌃⌘F" : "F11"})`}
      disabled={fullscreen.data === undefined}
      onClick={() => toggle.mutate(!on)}
      className={navItemClass}
    >
      <Icon className="size-5 shrink-0" aria-hidden />
      <span className="sr-only xl:not-sr-only xl:text-sm xl:font-medium">{label}</span>
    </button>
  );
}
