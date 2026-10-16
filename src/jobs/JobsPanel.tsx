import { useQueryClient } from "@tanstack/react-query";
import { Download, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { Button } from "../ui/Button";
import { JobList } from "./JobList";
import { isFinished, jobsKey, useJobs } from "./queries";

/** Sidebar entry for downloads and installs, with a pop-over list (PLAN.md §8.3). */
export function JobsPanel() {
  const { t } = useTranslation();
  const client = useQueryClient();
  const jobs = useJobs().data ?? [];
  const [open, setOpen] = useState(false);
  const panel = useRef<HTMLDivElement>(null);
  const running = jobs.filter((j) => !isFinished(j)).length;
  const failed = jobs.some((j) => j.phase === "failed");

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (
        e instanceof KeyboardEvent ? e.key === "Escape" : !panel.current?.contains(e.target as Node)
      )
        setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => {
      document.removeEventListener("mousedown", close);
      document.removeEventListener("keydown", close);
    };
  }, [open]);

  if (jobs.length === 0) return null;

  async function clearFinished() {
    await commands.clearFinishedJobs();
    client.invalidateQueries({ queryKey: jobsKey });
  }

  return (
    <div ref={panel} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        aria-expanded={open}
        title={t("jobs.title")}
        className="relative flex h-12 w-full items-center justify-center gap-3 rounded-lg text-muted transition hover:bg-surface-raised hover:text-ink xl:justify-start xl:px-3"
      >
        <Download className={`size-5 shrink-0 ${running ? "animate-pulse" : ""}`} aria-hidden />
        <span className="sr-only xl:not-sr-only xl:text-sm xl:font-medium">{t("jobs.title")}</span>
        {(running > 0 || failed) && (
          <span
            className={`absolute top-1.5 right-1.5 min-w-4 rounded-full px-1 font-pixel text-[0.5rem] leading-4 xl:static xl:ml-auto ${
              failed ? "bg-danger text-white" : "bg-accent text-accent-ink"
            }`}
          >
            {failed ? "!" : running}
          </span>
        )}
      </button>
      {open && (
        <div
          role="dialog"
          aria-label={t("jobs.title")}
          className="absolute bottom-0 left-full z-10 ml-2 grid w-80 gap-4 rounded-xl border border-border bg-surface p-4 shadow-card"
        >
          <div className="flex items-center justify-between">
            <h2 className="font-semibold">{t("jobs.title")}</h2>
            <Button variant="ghost" className="px-2" onClick={() => setOpen(false)}>
              <X className="size-4" aria-hidden />
              <span className="sr-only">{t("jobs.close")}</span>
            </Button>
          </div>
          <JobList jobs={jobs} />
          {jobs.some(isFinished) && (
            <Button variant="secondary" onClick={clearFinished}>
              {t("jobs.clearFinished")}
            </Button>
          )}
        </div>
      )}
    </div>
  );
}
