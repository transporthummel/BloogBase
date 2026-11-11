import { Gamepad2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { controllerConnected } from "../profile/controller";
import { standardPad } from "./usePad";

/** How often to ask whether a controller is there that the webview hasn't shown yet. */
const EVERY = 2000;

/**
 * The webview shows a controller only after a button was pressed in the
 * window. Windows (XInput) knows earlier: until then, this asks for a press.
 * Elsewhere `controllerConnected` says no, and nothing shows.
 */
export function PadHint() {
  const { t } = useTranslation();
  const [seen, setSeen] = useState(() => standardPad() !== null);
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    const update = () => setSeen(standardPad() !== null);
    window.addEventListener("gamepadconnected", update);
    window.addEventListener("gamepaddisconnected", update);
    return () => {
      window.removeEventListener("gamepadconnected", update);
      window.removeEventListener("gamepaddisconnected", update);
    };
  }, []);

  useEffect(() => {
    if (seen || typeof navigator.getGamepads !== "function") return;
    let gone = false;
    const ask = async () => {
      // While DOSBox is in front, the controller is for the game.
      if (!document.hasFocus()) return;
      const yes = await controllerConnected();
      if (!gone) setConnected(yes);
    };
    void ask();
    const timer = setInterval(() => void ask(), EVERY);
    return () => {
      gone = true;
      clearInterval(timer);
    };
  }, [seen]);

  if (seen || !connected) return null;
  return (
    <p
      role="status"
      className="pointer-events-none fixed bottom-4 left-1/2 z-40 flex -translate-x-1/2 items-center gap-2 rounded-full border border-border bg-surface px-4 py-2 text-sm shadow-card"
    >
      <Gamepad2 className="size-4 shrink-0 text-muted" aria-hidden />
      {t("pad.pressButton")}
    </p>
  );
}
