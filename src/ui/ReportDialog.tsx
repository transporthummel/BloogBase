import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Copy, ExternalLink } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import { Loading } from "./Loading";
import { Notice } from "./Notice";

/**
 * A report for a GitHub issue: shown, copied, and pasted by the player into
 * the issue it opens. Nothing is sent.
 */
export function ReportDialog({
  title,
  hint,
  paste,
  queryKey,
  load,
  issueUrl,
  onClose,
}: {
  title: string;
  /** What the report is for and what's in it. */
  hint: string;
  /** What to do with it in the issue. */
  paste: string;
  queryKey: readonly unknown[];
  load: () => Promise<string>;
  /** The new issue to open, e.g. with a title filled in. */
  issueUrl: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const text = useQuery({ queryKey, queryFn: load, gcTime: 0 });
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    if (text.data) {
      await navigator.clipboard.writeText(text.data);
      setCopied(true);
    }
  };
  const report = async () => {
    await copy();
    await openUrl(issueUrl);
  };
  return (
    <Dialog wide title={title} onClose={onClose}>
      <p className="max-w-prose text-sm">{hint}</p>
      <ol className="grid max-w-prose list-decimal gap-1 pl-5 text-sm">
        <li>{t("report.open")}</li>
        <li>{paste}</li>
      </ol>
      {text.isPending && <Loading />}
      {text.isError && <Notice tone="error">{String(text.error)}</Notice>}
      {text.data && (
        <details className="text-sm">
          <summary className="w-fit cursor-pointer text-muted">{t("report.show")}</summary>
          <pre className="mt-2 max-h-[45vh] overflow-auto rounded-lg bg-surface-raised p-3 text-xs whitespace-pre-wrap select-text">
            {text.data}
          </pre>
        </details>
      )}
      <div className="flex flex-wrap items-center justify-end gap-2">
        {/* The buttons' label change alone isn't announced. */}
        <span role="status" className="sr-only">
          {copied ? t("about.copied") : ""}
        </span>
        <Button variant="secondary" onClick={onClose}>
          {t("common.close")}
        </Button>
        <Button variant="secondary" disabled={!text.data} onClick={() => void copy()}>
          <Copy className="size-4" aria-hidden />
          {copied ? t("about.copied") : t("about.copy")}
        </Button>
        <Button disabled={!text.data} onClick={() => void report()}>
          <ExternalLink className="size-4" aria-hidden />
          {t("about.newIssue")}
        </Button>
      </div>
    </Dialog>
  );
}
