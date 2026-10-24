import { openUrl } from "@tauri-apps/plugin-opener";
import { Trans, useTranslation } from "react-i18next";
import { errorMessage } from "../errors";
import { isCommandError } from "../ipc";
import { Notice } from "../ui/Notice";

/** A failed download; where BloogBase can't open the file, offers it in the browser. */
export function DownloadError({ error, url }: { error: unknown; url: string }) {
  const { t } = useTranslation();
  return (
    <Notice tone="error">
      {isCommandError(error) && error.kind === "unsupportedDownload" ? (
        <Trans
          i18nKey="errors.unsupportedDownload"
          values={{ message: error.message }}
          components={{
            browser: (
              <button
                type="button"
                className="text-accent underline hover:text-ink"
                onClick={() => openUrl(url)}
              />
            ),
          }}
        />
      ) : (
        errorMessage(t, error)
      )}
    </Notice>
  );
}
