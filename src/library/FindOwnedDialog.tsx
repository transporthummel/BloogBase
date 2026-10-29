import { Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "../errors";
import { StoreLinks, StoreScanList } from "../setup/StoreScanList";
import { useStoreScan } from "../setup/storeScan";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { Notice } from "../ui/Notice";

/**
 * The setup wizard's Steam and GOG search on its own, for games bought
 * after setup: no folders or ZIP files to find by hand.
 */
export function FindOwnedDialog({
  onClose,
  onImported,
}: {
  onClose: () => void;
  /** How many games were added. */
  onImported: (count: number) => void;
}) {
  const { t } = useTranslation();
  const store = useStoreScan(onImported);
  const { count, importStores } = store;
  const error = store.scan.error ?? importStores.error;

  return (
    <Dialog
      title={t("setup.owned.title")}
      icon={<Search className="size-6 shrink-0 text-accent" aria-hidden />}
      onClose={onClose}
    >
      <p className="text-sm text-muted">{t("setup.owned.intro")}</p>
      <StoreScanList store={store} />
      {store.scan.data?.length === 0 && <StoreLinks />}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onClose}>
          {t("common.cancel")}
        </Button>
        <Button
          onClick={() => importStores.mutate()}
          disabled={count === 0 || importStores.isPending}
          data-autofocus
        >
          {t("setup.owned.import", { count })}
        </Button>
      </div>
    </Dialog>
  );
}
