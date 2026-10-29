import { Download } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { JobRow } from "../jobs/JobList";
import { useJobLabel } from "../jobs/label";
import { isFinished, useJob, useStartJob } from "../jobs/queries";
import { Button } from "../ui/Button";
import type { Variant } from "../ui/buttonClass";

/** Button that downloads the pinned DOSBox Staging build, with progress. */
export function InstallRuntimeButton({
  label,
  variant = "primary",
}: {
  label: string;
  variant?: Variant;
}) {
  const { t } = useTranslation();
  const jobLabel = useJobLabel();
  const job = useJob("runtime");
  const start = useStartJob(() => commands.installRuntime());
  const busy = start.isPending || (!!job && !isFinished(job));

  return (
    <div className="grid gap-3">
      <div>
        <Button variant={variant} onClick={() => start.mutate(undefined)} disabled={busy}>
          <Download className="size-4" aria-hidden />
          {job?.phase === "failed" ? t("jobs.retry") : label}
        </Button>
      </div>
      {job && (
        <div className="grid gap-1.5">
          <JobRow job={job} label={jobLabel(job)} />
        </div>
      )}
    </div>
  );
}
