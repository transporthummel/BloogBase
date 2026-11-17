import { useMutation, useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import { ChevronDown, ChevronLeft, ChevronRight, Copy, FolderOpen, Trash2 } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type Profile } from "../bindings";
import { errorMessage } from "../errors";
import { formatDateTime } from "../format";
import { unwrap } from "../ipc";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { EmptyState } from "../ui/EmptyState";
import { Loading } from "../ui/Loading";
import { Menu } from "../ui/Menu";
import { Notice } from "../ui/Notice";
import { useCopyCaptures } from "./CopyCaptures";

type Capture = { file: string; takenAt: number | null };

/**
 * What the player captured in DOSBox, as a grid with a viewer (§8.3). The
 * texts come from the `kind` section of the translations.
 */
export function CapturesPanel<T extends Capture>({
  profile,
  kind,
  hotkey,
  captures,
  queryKey,
  remove: removeFile,
  tile,
  viewer,
}: {
  profile: Profile;
  kind: "screenshots" | "videos";
  hotkey: string;
  captures: UseQueryResult<T[]>;
  queryKey: readonly unknown[];
  remove: (file: string) => Promise<unknown>;
  /** Inside the thumbnail button. */
  tile: (capture: T) => ReactNode;
  viewer: (capture: T) => ReactNode;
}) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [viewing, setViewingAt] = useState<number | null>(null);
  // Deleting takes a second click.
  const [confirming, setConfirming] = useState(false);
  const setViewing = (i: number | null) => {
    setViewingAt(i);
    setConfirming(false);
  };

  // Only this tab's kind.
  const copy = useCopyCaptures([profile.id], kind === "screenshots" ? "screenshot" : "video");
  const openFolder = useMutation({
    mutationFn: () => unwrap(commands.openCaptureFolder(profile.id)),
  });
  const remove = useMutation({
    mutationFn: removeFile,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey });
      setViewing(null);
    },
  });

  const list = captures.data ?? [];
  const shown = viewing === null ? undefined : list[viewing];
  const error = captures.error ?? openFolder.error ?? remove.error;
  const title = (c: T) => (c.takenAt ? formatDateTime(i18n.language, c.takenAt) : c.file);

  return (
    <div className="grid gap-4">
      {/* Without any, the empty state says the same and the folder is empty. */}
      {list.length > 0 && (
        // Narrow, the two buttons fold into one menu beside the hint.
        <div className="@container">
          <div className="flex items-center gap-3">
            <p className="mr-auto text-sm text-muted">{t(`${kind}.hint`, { key: hotkey })}</p>
            <div className="hidden shrink-0 gap-2 @2xl:flex">
              {copy.button}
              <Button variant="secondary" onClick={() => openFolder.mutate()}>
                <FolderOpen className="size-4" aria-hidden />
                {t("screenshots.openFolder")}
              </Button>
            </div>
            <div className="shrink-0 @2xl:hidden">
              <Menu
                label={t("screenshots.folderMenu")}
                trigger={
                  <>
                    <FolderOpen className="size-4" aria-hidden />
                    {t("screenshots.folderMenu")}
                    <ChevronDown className="size-4" aria-hidden />
                  </>
                }
                items={[
                  {
                    label: t("copyCaptures.copy"),
                    icon: <Copy className="size-4" />,
                    onSelect: copy.start,
                    disabled: copy.pending,
                  },
                  {
                    label: t("screenshots.openFolder"),
                    icon: <FolderOpen className="size-4" />,
                    onSelect: () => openFolder.mutate(),
                  },
                ]}
              />
            </div>
          </div>
        </div>
      )}
      {copy.result}
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      {captures.isPending && <Loading />}
      {captures.data?.length === 0 && (
        <EmptyState title={t(`${kind}.emptyTitle`)}>
          <p className="max-w-md text-sm text-muted">{t(`${kind}.empty`, { key: hotkey })}</p>
        </EmptyState>
      )}
      <ul className="grid grid-cols-[repeat(auto-fill,minmax(12rem,1fr))] gap-3">
        {list.map((capture, i) => (
          <li key={capture.file}>
            <button
              type="button"
              onClick={() => setViewing(i)}
              className="relative block w-full overflow-hidden rounded-lg border border-border bg-black transition hover:border-muted"
              aria-label={t(`${kind}.view`, { date: title(capture) })}
            >
              {tile(capture)}
            </button>
          </li>
        ))}
      </ul>

      {shown && viewing !== null && (
        <Dialog wide title={title(shown)} onClose={() => setViewing(null)}>
          {viewer(shown)}
          <div className="flex flex-wrap items-center gap-2">
            <Button
              variant="ghost"
              onClick={() => setViewing(viewing - 1)}
              disabled={viewing === 0}
              aria-label={t(`${kind}.newer`)}
            >
              <ChevronLeft className="size-4" aria-hidden />
            </Button>
            <span className="text-sm text-muted">
              {t("screenshots.position", { n: viewing + 1, count: list.length })}
            </span>
            <Button
              variant="ghost"
              onClick={() => setViewing(viewing + 1)}
              disabled={viewing === list.length - 1}
              aria-label={t(`${kind}.older`)}
            >
              <ChevronRight className="size-4" aria-hidden />
            </Button>
            <Button
              variant="secondary"
              className="ml-auto"
              onClick={() => (confirming ? remove.mutate(shown.file) : setConfirming(true))}
              disabled={remove.isPending}
            >
              <Trash2 className="size-4" aria-hidden />
              {confirming ? t("screenshots.confirmDelete") : t("screenshots.delete")}
            </Button>
          </div>
        </Dialog>
      )}
    </div>
  );
}
