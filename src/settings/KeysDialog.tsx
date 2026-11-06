import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { GameKeys } from "../bindings";
import { errorMessage } from "../errors";
import { Keycap } from "../profile/Keycap";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";
import { useGameSettings } from "./queries";
import { clashesWithDefault, isReserved, keyId, scancodeFor } from "./scancodes";

const ACTIONS: (keyof GameKeys)[] = ["up", "down", "left", "right", "jump", "pogo", "fire"];

const NO_KEYS: GameKeys = {
  up: null,
  down: null,
  left: null,
  right: null,
  jump: null,
  pogo: null,
  fire: null,
};

type Message = { tone: "error" | "info"; text: string };

/** Chooses the keys for all games, one at a time, by pressing them. */
export function KeysDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const { settings, update, error } = useGameSettings();
  const [waiting, setWaiting] = useState<keyof GameKeys | null>(null);
  const [message, setMessage] = useState<Message | null>(null);
  const keys = settings?.keys;

  useEffect(() => {
    if (!waiting || !keys) return;
    const action = waiting;
    function press(e: KeyboardEvent) {
      if (!keys) return;
      // Before the dialog's own Esc, and before the webview does anything with it.
      e.preventDefault();
      e.stopImmediatePropagation();
      setWaiting(null);
      if (e.key === "Escape") {
        setMessage(null);
        return;
      }
      const scancode = scancodeFor(e);
      if (scancode === null) {
        setMessage({ tone: "error", text: t("settings.keysUnknown") });
      } else if (isReserved(scancode)) {
        setMessage({ tone: "error", text: t("settings.keysReserved") });
      } else {
        const taken = ACTIONS.find((other) => other !== action && keys[other] === scancode);
        if (taken) {
          setMessage({
            tone: "error",
            text: t("settings.keysTaken", { action: t(`settings.keyAction.${taken}`) }),
          });
          return;
        }
        setMessage(
          clashesWithDefault(action, scancode, keys)
            ? { tone: "info", text: t("settings.keysClash") }
            : null,
        );
        update({ keys: { ...keys, [action]: scancode } });
      }
    }
    document.addEventListener("keydown", press, true);
    return () => document.removeEventListener("keydown", press, true);
  }, [waiting, keys, t, update]);

  return (
    <Dialog
      title={t("settings.keysTitle")}
      onClose={onClose}
      footer={
        <div className="flex flex-wrap items-center gap-3">
          {keys && ACTIONS.some((action) => keys[action] !== null) && (
            <Button
              variant="ghost"
              onClick={() => {
                setMessage(null);
                setWaiting(null);
                update({ keys: NO_KEYS });
              }}
            >
              {t("settings.keysResetAll")}
            </Button>
          )}
          <Button className="ml-auto" variant="secondary" onClick={onClose}>
            {t("common.close")}
          </Button>
        </div>
      }
    >
      <p className="text-sm text-muted">{t("settings.keysIntro")}</p>
      {keys && (
        <ul className="grid gap-2">
          {ACTIONS.map((action) => {
            const key = keys[action];
            return (
              <li key={action} className="flex flex-wrap items-center gap-3 text-sm">
                <span className="w-24">{t(`settings.keyAction.${action}`)}</span>
                <span className="mr-auto">
                  {waiting === action ? (
                    <span role="status" className="text-muted">
                      {t("settings.keysPress")}
                    </span>
                  ) : key === null ? (
                    <span className="text-muted">{t("settings.setByGame")}</span>
                  ) : (
                    <Keycap id={keyId(key)} changed />
                  )}
                </span>
                {waiting !== action && (
                  <Button
                    variant="secondary"
                    onClick={() => {
                      setMessage(null);
                      setWaiting(action);
                    }}
                  >
                    {t("settings.keysChange")}
                  </Button>
                )}
                {key !== null && waiting !== action && (
                  <Button
                    variant="ghost"
                    onClick={() => {
                      setMessage(null);
                      update({ keys: { ...keys, [action]: null } });
                    }}
                  >
                    {t("settings.keysReset")}
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {message && <Notice tone={message.tone}>{message.text}</Notice>}
      <p className="max-w-prose text-xs text-muted">{t("settings.keysHint")}</p>
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
    </Dialog>
  );
}
