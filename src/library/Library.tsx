import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { type TFunction } from "i18next";
import { FileArchive, FolderOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { commands, type Game, type Install } from "../bindings";
import { isCommandError, unwrap } from "../ipc";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { EmptyState } from "../ui/EmptyState";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";

export function Library() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
  const installs = useQuery({
    queryKey: ["installs"],
    queryFn: () => unwrap(commands.listInstalls()),
  });

  const importGames = useMutation({
    mutationFn: (path: string) => unwrap(commands.importGames(path)),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["installs"] }),
  });

  async function pick(kind: "folder" | "zip") {
    const path = await open(
      kind === "folder" ? { directory: true } : { filters: [{ name: "ZIP", extensions: ["zip"] }] },
    );
    if (path) importGames.mutate(path);
  }

  const importButtons = (
    <>
      <Button onClick={() => pick("folder")} disabled={importGames.isPending}>
        <FolderOpen className="size-4" aria-hidden />
        {t("library.importFolder")}
      </Button>
      <Button variant="secondary" onClick={() => pick("zip")} disabled={importGames.isPending}>
        <FileArchive className="size-4" aria-hidden />
        {t("library.importZip")}
      </Button>
    </>
  );

  const gameById = new Map(games.data?.map((g) => [g.id, g]));
  const error = importGames.error ?? installs.error;
  const empty = installs.data?.length === 0;

  return (
    <>
      <PageHeader title={t("library.title")} actions={!empty && importButtons} />

      <div className="mb-4">
        {importGames.isPending && <Notice>{t("library.importing")}</Notice>}
        {importGames.isSuccess && (
          <Notice>{t("library.imported", { count: importGames.data.length })}</Notice>
        )}
        {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      </div>

      {empty && (
        <EmptyState title={t("library.emptyTitle")}>
          <p className="max-w-md text-sm text-muted">{t("library.empty")}</p>
          <div className="flex flex-wrap justify-center gap-3">{importButtons}</div>
        </EmptyState>
      )}

      <ul className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4">
        {installs.data?.map((install) => (
          <li key={install.id}>
            <InstallCard install={install} game={gameById.get(install.gameId)} />
          </li>
        ))}
      </ul>
    </>
  );
}

const engineColor = {
  vorticons: "bg-vorticons",
  galaxy: "bg-galaxy",
  dreams: "bg-dreams",
} as const;

function InstallCard({ install, game }: { install: Install; game?: Game }) {
  const { t } = useTranslation();
  const version = game?.versions.find((v) => v.id === install.versionId);
  // Placeholder cover: episode number on the engine's color.
  const episode = install.gameId.replace(/^keen/, "").replace(/^kdreams$/, "D");
  return (
    <Card>
      <div
        className={`flex aspect-[3/4] items-center justify-center font-pixel text-5xl text-white/90 ${
          game ? engineColor[game.engine] : "bg-surface-raised"
        }`}
        aria-hidden
      >
        {episode}
      </div>
      <div className="grid gap-2 p-3">
        <h2 className="text-sm leading-tight font-semibold">{install.label}</h2>
        <div>
          <Badge>{version?.label ?? t("library.unknownVersion")}</Badge>
        </div>
      </div>
    </Card>
  );
}

function errorMessage(t: TFunction, err: unknown): string {
  if (isCommandError(err)) {
    if (err.kind === "noGamesFound" || err.kind === "unsupportedSource" || err.kind === "archive") {
      return t(`errors.${err.kind}`);
    }
    return t("errors.generic", { message: err.message });
  }
  return t("errors.generic", { message: String(err) });
}
