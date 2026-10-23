import { useMutation } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FileText } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Readme } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { linkify } from "./readme";

/**
 * A mod's README, with a picker for its other documents. Shown as plain text
 * only (React escapes it); links open in the browser, never in the app's
 * window. Web pages are never shown here, only opened in the browser.
 * The mod is installed (`modId`) or still a folder or ZIP (`path`).
 */
export function ReadmePanel({
  readmes,
  ...from
}: { readmes: Readme[] } & ({ modId: string } | { path: string })) {
  const { t } = useTranslation();
  const pickerId = useId();
  const [file, setFile] = useState(readmes[0].file);
  const readme = readmes.find((r) => r.file === file) ?? readmes[0];
  const openPage = useMutation({
    mutationFn: (file: string) =>
      unwrap(
        "modId" in from
          ? commands.openModReadme(from.modId, file)
          : commands.openReadmeAt(from.path, file),
      ),
  });
  return (
    <Card>
      <div className="flex items-center gap-2 border-b border-border px-4 py-2 text-xs text-muted">
        <FileText className="size-4 shrink-0" aria-hidden />
        {readmes.length > 1 ? (
          <>
            <label htmlFor={pickerId} className="sr-only">
              {t("readme.file")}
            </label>
            <select
              id={pickerId}
              className="min-w-0 rounded-lg border border-border bg-surface-raised px-2 py-1 text-xs text-ink"
              value={readme.file}
              onChange={(e) => setFile(e.target.value)}
            >
              {readmes.map((r) => (
                <option key={r.file} value={r.file}>
                  {r.file}
                </option>
              ))}
            </select>
          </>
        ) : (
          <p>
            <span className="sr-only">{t("readme.file")}</span>
            <span className="break-all">{readme.file}</span>
          </p>
        )}
      </div>
      {readme.html ? (
        <div className="grid justify-items-start gap-3 p-4">
          <p className="text-sm text-muted">{t("readme.webPage")}</p>
          <Button variant="secondary" onClick={() => openPage.mutate(readme.file)}>
            <ExternalLink className="size-4" aria-hidden />
            {t("readme.openInBrowser")}
          </Button>
          {openPage.error && <Notice tone="error">{errorMessage(t, openPage.error)}</Notice>}
        </div>
      ) : (
        <pre className="p-4 font-mono text-sm leading-snug break-words whitespace-pre-wrap">
          {linkify(readme.text).map((part, i) =>
            "url" in part ? (
              <button
                key={i}
                type="button"
                onClick={() => openUrl(part.url)}
                className="text-left break-all text-accent underline hover:text-ink"
              >
                {part.url}
              </button>
            ) : (
              part.text
            ),
          )}
        </pre>
      )}
    </Card>
  );
}
