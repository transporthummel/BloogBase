import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { type TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import { commands, type Game, type Install } from "../bindings";
import { isCommandError, unwrap } from "../ipc";

export function Library() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const games = useQuery({ queryKey: ["games"], queryFn: commands.listGames, staleTime: Infinity });
  const installs = useQuery({ queryKey: ["installs"], queryFn: () => unwrap(commands.listInstalls()) });

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

  const gameById = new Map(games.data?.map((g) => [g.id, g]));
  const error = importGames.error ?? installs.error;

  return (
    <main className="mx-auto max-w-6xl p-4 sm:p-8">
      <header className="mb-6 flex flex-wrap items-center gap-3">
        <h1 className="mr-auto text-2xl font-bold">{t("library.title")}</h1>
        <ImportButton onClick={() => pick("folder")} disabled={importGames.isPending}>
          {t("library.importFolder")}
        </ImportButton>
        <ImportButton onClick={() => pick("zip")} disabled={importGames.isPending}>
          {t("library.importZip")}
        </ImportButton>
      </header>

      <div role="status" className="mb-4 text-sm">
        {importGames.isPending && <p className="text-muted">{t("library.importing")}</p>}
        {importGames.isSuccess && (
          <p className="text-muted">{t("library.imported", { count: importGames.data.length })}</p>
        )}
        {error && <p className="text-accent">{errorMessage(t, error)}</p>}
      </div>

      {installs.data?.length === 0 && <p className="text-muted">{t("library.empty")}</p>}

      <ul className="grid grid-cols-[repeat(auto-fill,minmax(10rem,1fr))] gap-4">
        {installs.data?.map((install) => (
          <li key={install.id}>
            <InstallCard install={install} game={gameById.get(install.gameId)} />
          </li>
        ))}
      </ul>
    </main>
  );
}

function ImportButton(props: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      type="button"
      className="rounded-lg bg-accent px-4 py-2 font-semibold text-white shadow-sm transition hover:brightness-110 disabled:opacity-50"
      {...props}
    />
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
    <article className="overflow-hidden rounded-xl bg-card shadow-sm">
      <div
        className={`flex aspect-[3/4] items-center justify-center text-6xl font-black text-white/90 ${
          game ? engineColor[game.engine] : "bg-muted"
        }`}
        aria-hidden
      >
        {episode}
      </div>
      <div className="p-3">
        <h2 className="text-sm leading-tight font-semibold">{install.label}</h2>
        <p className="mt-1 text-xs text-muted">{version?.label ?? t("library.unknownVersion")}</p>
      </div>
    </article>
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
