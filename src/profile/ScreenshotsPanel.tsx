import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { unwrap } from "../ipc";
import { CapturesPanel } from "./CapturesPanel";
import { screenshotSrc, screenshotsKey, useScreenshots } from "./captures";

/** DOSBox Staging's screenshot hotkey. */
function hotkey(): string {
  return /Mac/.test(navigator.userAgent) ? "⌘ + F5" : "Ctrl + F5";
}

/** Screenshots the player took in DOSBox, with a viewer (§8.3). */
export function ScreenshotsPanel({ profile }: { profile: Profile }) {
  const { t } = useTranslation();
  return (
    <CapturesPanel
      profile={profile}
      kind="screenshots"
      hotkey={hotkey()}
      captures={useScreenshots(profile.id)}
      queryKey={screenshotsKey(profile.id)}
      remove={(file) => unwrap(commands.deleteScreenshot(profile.id, file))}
      tile={(shot) => (
        <img
          src={screenshotSrc(shot)}
          alt=""
          loading="lazy"
          className="aspect-[4/3] w-full object-contain [image-rendering:pixelated]"
        />
      )}
      viewer={(shot) => (
        <img
          src={screenshotSrc(shot)}
          alt={t("screenshots.alt", { game: profile.label })}
          className="max-h-[70vh] w-full rounded bg-black object-contain [image-rendering:pixelated]"
        />
      )}
    />
  );
}
