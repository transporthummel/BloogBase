import { useMutation } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type Kind } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";

/**
 * "Copy to a folder…" for the screenshots and videos of profiles (with
 * `kind`, only those): each goes into a folder named after its game or mod
 * in the folder the player picks. The button and what came of it, to be
 * placed apart, and `start` for a menu item.
 */
export function useCopyCaptures(profileIds: string[], kind: Kind | null = null) {
  const { t } = useTranslation();
  const copy = useMutation({
    mutationFn: async () => {
      const dest = await open({ directory: true });
      if (!dest) return null;
      return { dest, ...(await unwrap(commands.copyCaptures(profileIds, kind, dest))) };
    },
  });
  const done = copy.data;
  const files = done && t("copyCaptures.files", { count: done.files });

  const button = (
    <Button variant="secondary" disabled={copy.isPending} onClick={() => copy.mutate()}>
      <Copy className="size-4" aria-hidden />
      {t("copyCaptures.copy")}
    </Button>
  );
  const result = (
    <>
      {done && (
        <Notice>
          {/* One folder: where the files are; several: where the folders are. */}
          {done.folders.length === 1
            ? t("copyCaptures.copiedOne", { files })
            : t("copyCaptures.copiedEach", { files })}{" "}
          <span className="select-text">
            {done.folders.length === 1 ? done.folders[0] : done.dest}
          </span>
        </Notice>
      )}
      {copy.error && <Notice tone="error">{errorMessage(t, copy.error)}</Notice>}
    </>
  );
  return { button, result, start: () => copy.mutate(), pending: copy.isPending };
}
