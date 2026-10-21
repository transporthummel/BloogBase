import { openUrl } from "@tauri-apps/plugin-opener";
import { FileText } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Readme } from "../bindings";
import { Card } from "../ui/Card";
import { linkify } from "./readme";

/**
 * A mod's README, with a picker for its other documents. Shown as plain text
 * only (React escapes it); links open in the browser, never in the app's
 * window.
 */
export function ReadmePanel({ readmes }: { readmes: Readme[] }) {
  const { t } = useTranslation();
  const pickerId = useId();
  const [file, setFile] = useState(readmes[0].file);
  const readme = readmes.find((r) => r.file === file) ?? readmes[0];
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
    </Card>
  );
}
