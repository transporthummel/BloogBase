import { type TFunction } from "i18next";
import type { ErrorKind } from "./bindings";
import { isCommandError } from "./ipc";

/** Error kinds with their own friendly message; everything else is generic. */
const translated: ErrorKind[] = [
  "noGamesFound",
  "unsupportedSource",
  "archive",
  "network",
  "checksumMismatch",
  "unsupportedPlatform",
  "runtimeMissing",
  "invalidRuntime",
  "busy",
  "alreadyRunning",
  "invalidLocation",
  "noModFound",
  "incompatibleInstall",
  "missingGame",
  "badSignature",
];

export function errorMessage(t: TFunction, err: unknown): string {
  if (isCommandError(err)) {
    if (translated.includes(err.kind)) {
      return t(`errors.${err.kind}`, { message: err.message });
    }
    return t("errors.generic", { message: err.message });
  }
  return t("errors.generic", { message: String(err) });
}
