import { openUrl } from "@tauri-apps/plugin-opener";
import { FileText } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Readme } from "../bindings";
import { Card } from "../ui/Card";
import { linkify } from "./readme";

/**
 * A mod's README. Shown as plain text only (React escapes it); links open in
 * the browser, never in the app's window.
 */
export function ReadmePanel({ readme }: { readme: Readme }) {
  const { t } = useTranslation();
  return (
    <Card>
      <p className="flex items-center gap-2 border-b border-border px-4 py-2 text-xs text-muted">
        <FileText className="size-4" aria-hidden />
        <span className="sr-only">{t("readme.file")}</span>
        <span className="break-all">{readme.file}</span>
      </p>
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
