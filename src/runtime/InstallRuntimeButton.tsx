import { Download } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, events, type RuntimeInstallProgress } from "../bindings";
import { errorMessage } from "../errors";
import { formatMegabytes } from "../format";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { Progress } from "../ui/Progress";
import { useRuntimeMutation } from "./queries";

/** Button that downloads the pinned DOSBox Staging build, with progress. */
export function InstallRuntimeButton({ label }: { label: string }) {
  const { t } = useTranslation();
  const [progress, setProgress] = useState<RuntimeInstallProgress | null>(null);
  const install = useRuntimeMutation(() => unwrap(commands.installRuntime()));

  useEffect(() => {
    const unlisten = events.runtimeInstallProgress.listen((e) => setProgress(e.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  return (
    <div className="grid gap-3">
      <div>
        <Button
          onClick={() => {
            setProgress(null);
            install.mutate(undefined);
          }}
          disabled={install.isPending}
        >
          <Download className="size-4" aria-hidden />
          {label}
        </Button>
      </div>
      {install.isPending && (
        <div className="grid gap-1.5">
          <Progress
            label={t("runtime.downloading")}
            value={progress?.phase === "downloading" ? progress.received : undefined}
            max={progress?.phase === "downloading" ? progress.total : undefined}
          />
          <Notice>
            {progress?.phase === "extracting"
              ? t("runtime.extracting")
              : progress
                ? t("runtime.downloadProgress", {
                    received: formatMegabytes(progress.received),
                    total: formatMegabytes(progress.total),
                  })
                : t("runtime.downloading")}
          </Notice>
        </div>
      )}
      {install.isSuccess && <Notice>{t("runtime.installed")}</Notice>}
      {install.error && <Notice tone="error">{errorMessage(t, install.error)}</Notice>}
    </div>
  );
}
