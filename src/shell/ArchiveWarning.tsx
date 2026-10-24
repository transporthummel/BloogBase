import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { events, type ArchiveDamaged } from "../bindings";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";

/** Warns when files imported from a ZIP failed their checksum. */
export function ArchiveWarning() {
  const { t } = useTranslation();
  // One import after another can each have damaged files.
  const [queue, setQueue] = useState<ArchiveDamaged[]>([]);

  useEffect(() => {
    const unlisten = events.archiveDamaged.listen((e) => setQueue((q) => [...q, e.payload]));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const current = queue[0];
  if (!current) return null;
  const close = () => setQueue((q) => q.slice(1));
  const count = current.files.length;
  return (
    <Dialog title={t("archiveDamaged.title", { count })} onClose={close}>
      <p className="text-sm">{t("archiveDamaged.body", { count, name: current.name })}</p>
      <ul className="list-disc pl-6 font-mono text-sm">
        {current.files.map((f) => (
          <li key={f}>{f}</li>
        ))}
      </ul>
      <p className="text-sm text-muted">{t("archiveDamaged.hint", { count })}</p>
      <div className="flex justify-end">
        <Button onClick={close} data-autofocus>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}
