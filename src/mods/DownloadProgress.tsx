import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { events } from "../bindings";
import { formatMegabytes } from "../format";
import { Progress } from "../ui/Progress";

/** A running download of a KeenWiki link, then the check of its files. */
export function DownloadProgress({ url }: { url: string }) {
  const { t } = useTranslation();
  const [bytes, setBytes] = useState({ received: 0, total: 0 });
  useEffect(() => {
    const unlisten = events.keenwikiProgress.listen(({ payload }) => {
      if (payload.url === url) setBytes(payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [url]);

  const { received, total } = bytes;
  const known = total > 0;
  const checking = known && received >= total;
  return (
    <div className="grid w-full gap-1.5">
      <p className="text-sm">{t("keenwiki.downloading")}</p>
      <Progress
        label={t("keenwiki.downloading")}
        value={known ? received : undefined}
        max={known ? total : undefined}
      />
      <p role="status" className="text-xs text-muted">
        {checking
          ? t("keenwiki.checking")
          : known
            ? t("jobs.progress", {
                received: formatMegabytes(received),
                total: formatMegabytes(total),
              })
            : received > 0
              ? t("keenwiki.received", { received: formatMegabytes(received) })
              : t("jobs.starting")}
      </p>
    </div>
  );
}
