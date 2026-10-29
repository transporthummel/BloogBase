import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "./Button";
import { Dialog } from "./Dialog";

/**
 * Asks before deleting or resetting something; `confirm` is the button that
 * does it. Cancel has the focus, so Enter keeps everything as it is.
 */
export function ConfirmDialog({
  title,
  confirm,
  busy = false,
  onConfirm,
  onClose,
  children,
}: {
  title: string;
  confirm: string;
  busy?: boolean;
  onConfirm: () => void;
  onClose: () => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <Dialog title={title} onClose={onClose}>
      {children}
      <div className="flex justify-end gap-3">
        <Button variant="ghost" onClick={onClose} data-autofocus>
          {t("common.cancel")}
        </Button>
        <Button variant="danger" onClick={onConfirm} disabled={busy}>
          {confirm}
        </Button>
      </div>
    </Dialog>
  );
}
