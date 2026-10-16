import { CircleAlert, CircleCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Job } from "../bindings";
import { errorMessage } from "../errors";
import { formatMegabytes } from "../format";
import { Progress } from "../ui/Progress";
import { useJobLabel } from "./label";

/** One line per job with its progress or outcome. */
export function JobList({ jobs }: { jobs: Job[] }) {
  const label = useJobLabel();
  return (
    <ul className="grid gap-3">
      {jobs.map((job) => (
        <li key={job.id} className="grid gap-1.5">
          <JobRow job={job} label={label(job)} />
        </li>
      ))}
    </ul>
  );
}

export function JobRow({ job, label }: { job: Job; label: string }) {
  const { t } = useTranslation();
  if (job.phase === "done") {
    return (
      <p className="flex items-center gap-2 text-sm">
        <CircleCheck className="size-4 shrink-0 text-accent" aria-hidden />
        <span>{t("jobs.done", { label })}</span>
      </p>
    );
  }
  if (job.phase === "failed") {
    return (
      <div className="grid gap-1 text-sm">
        <p className="flex items-center gap-2">
          <CircleAlert className="size-4 shrink-0 text-danger" aria-hidden />
          <span>{t("jobs.failed", { label })}</span>
        </p>
        {job.error && <p className="text-xs text-muted">{errorMessage(t, job.error)}</p>}
      </div>
    );
  }
  const downloading = job.phase === "downloading" && job.total > 0;
  return (
    <>
      <p className="text-sm">{label}</p>
      <Progress
        label={label}
        value={downloading ? job.received : undefined}
        max={downloading ? job.total : undefined}
      />
      <p role="status" className="text-xs text-muted">
        {job.phase === "installing"
          ? t("jobs.installing")
          : downloading
            ? t("jobs.progress", {
                received: formatMegabytes(job.received),
                total: formatMegabytes(job.total),
              })
            : t("jobs.starting")}
      </p>
    </>
  );
}
