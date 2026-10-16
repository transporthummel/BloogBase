import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { CircleCheck, FileArchive, FileCog, FolderOpen, PartyPopper } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type FreeEpisode, type StoreInstall } from "../bindings";
import { gameTitle, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatMegabytes } from "../format";
import { unwrap } from "../ipc";
import { JobList, JobRow } from "../jobs/JobList";
import { useJobLabel } from "../jobs/label";
import { isFinished, useJobs, useStartJob } from "../jobs/queries";
import { profilesKey } from "../library/queries";
import { setLanguage, type Language } from "../preferences";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { useRuntimeMutation, useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { setupStatusKey, useFreeEpisodes, useSetupStatus } from "./queries";

export type StepProps = {
  next: () => void;
  back?: () => void;
  finish: () => void;
  finishing: boolean;
};

const STORE_LINKS = {
  gog: "https://www.gog.com/game/commander_keen_complete_pack",
  steam: "https://store.steampowered.com/app/9180/",
};

function StepHeader({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="grid gap-2">
      <h1 className="font-pixel text-base leading-relaxed">{title}</h1>
      {children && <div className="grid gap-2 text-sm text-muted">{children}</div>}
    </div>
  );
}

function Footer({
  back,
  skip,
  children,
}: {
  back?: () => void;
  skip?: () => void;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-3 border-t border-border pt-5">
      {back && (
        <Button variant="ghost" onClick={back}>
          {t("setup.back")}
        </Button>
      )}
      <div className="ml-auto flex flex-wrap gap-3">
        {skip && (
          <Button variant="secondary" onClick={skip}>
            {t("setup.skip")}
          </Button>
        )}
        {children}
      </div>
    </div>
  );
}

function OfflineNotice() {
  const { t } = useTranslation();
  return navigator.onLine === false ? <Notice tone="error">{t("setup.offline")}</Notice> : null;
}

// 1. Welcome --------------------------------------------------------------

export function WelcomeStep({ next }: StepProps) {
  const { t, i18n } = useTranslation();
  const languages: [Language, string][] = [
    ["en", "English"],
    ["de", "Deutsch"],
  ];
  return (
    <>
      <StepHeader title={t("setup.welcome.title")}>
        <p>{t("setup.welcome.intro")}</p>
        <p>{t("setup.welcome.steps")}</p>
      </StepHeader>
      <div role="group" aria-label={t("settings.language")} className="flex flex-wrap gap-3">
        {languages.map(([lng, name]) => (
          <Button
            key={lng}
            variant={i18n.language === lng ? "primary" : "secondary"}
            aria-pressed={i18n.language === lng}
            onClick={() => {
              setLanguage(lng);
              i18n.changeLanguage(lng);
            }}
          >
            {name}
          </Button>
        ))}
      </div>
      <Footer>
        <Button onClick={next}>{t("setup.welcome.start")}</Button>
      </Footer>
    </>
  );
}

// 2. Where to store games -------------------------------------------------

export function StorageStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });
  const setup = useSetupStatus();
  const move = useMutation({
    mutationFn: (path: string | null) => unwrap(commands.setDataLocation(path)),
    // The data folder is fixed while the app runs; start over in the new one.
    onSuccess: () => commands.restartApp(),
  });
  const custom = info.data && info.data.dataDir !== info.data.defaultDataDir;
  const locked = setup.data?.hasInstalls;

  async function choose() {
    const path = await open({ directory: true, title: t("setup.storage.choose") });
    if (path) move.mutate(path);
  }

  return (
    <>
      <StepHeader title={t("setup.storage.title")}>
        <p>{t("setup.storage.intro")}</p>
      </StepHeader>
      <p className="rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm break-all">
        {info.data?.dataDir}
      </p>
      <p className="text-sm text-muted">{t("setup.storage.usage")}</p>
      <details className="text-sm">
        <summary className="cursor-pointer text-muted">{t("setup.advanced")}</summary>
        <div className="mt-3 grid gap-3">
          {locked ? (
            <Notice>{t("setup.storage.locked")}</Notice>
          ) : (
            <div className="flex flex-wrap gap-3">
              <Button variant="secondary" onClick={choose} disabled={move.isPending}>
                <FolderOpen className="size-4" aria-hidden />
                {t("setup.storage.choose")}
              </Button>
              {custom && (
                <Button variant="ghost" onClick={() => move.mutate(null)} disabled={move.isPending}>
                  {t("setup.storage.useDefault")}
                </Button>
              )}
            </div>
          )}
          {move.isPending && <Notice>{t("setup.storage.restarting")}</Notice>}
          {move.error && <Notice tone="error">{errorMessage(t, move.error)}</Notice>}
        </div>
      </details>
      <Footer back={back}>
        <Button onClick={next}>{t("setup.next")}</Button>
      </Footer>
    </>
  );
}

// 3. Game engine ----------------------------------------------------------

export function EngineStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const status = useRuntimeStatus();
  const jobs = useJobs().data;
  const setCustom = useRuntimeMutation((path: string) => unwrap(commands.setCustomRuntime(path)));
  const st = status.data;
  const started = jobs?.some((j) => j.kind === "runtime");

  async function chooseCustom() {
    const path = await open({ title: t("runtime.chooseCustom") });
    if (path) setCustom.mutate(path);
  }

  return (
    <>
      <StepHeader title={t("setup.engine.title")}>
        <p>{t("runtime.needed")}</p>
      </StepHeader>
      {st?.active ? (
        <p className="flex items-center gap-2 text-sm">
          <CircleCheck className="size-4 shrink-0 text-accent" aria-hidden />
          {st.active.managed
            ? t("runtime.activeManaged", { version: st.active.version })
            : t("runtime.activeCustom", { version: st.active.version })}
        </p>
      ) : st?.downloadAvailable ? (
        <>
          <OfflineNotice />
          <InstallRuntimeButton label={t("runtime.install", { version: st.pinnedVersion })} />
          {started && <p className="text-sm text-muted">{t("setup.engine.continue")}</p>}
        </>
      ) : (
        st && <Notice>{t("errors.unsupportedPlatform")}</Notice>
      )}
      <details className="text-sm">
        <summary className="cursor-pointer text-muted">{t("setup.advanced")}</summary>
        <div className="mt-3 grid gap-3">
          <div>
            <Button variant="secondary" onClick={chooseCustom} disabled={setCustom.isPending}>
              <FileCog className="size-4" aria-hidden />
              {t("runtime.chooseCustom")}
            </Button>
          </div>
          <p className="text-xs text-muted">{t("runtime.customHint")}</p>
          {setCustom.error && <Notice tone="error">{errorMessage(t, setCustom.error)}</Notice>}
        </div>
      </details>
      <Footer back={back} skip={st?.active || started ? undefined : next}>
        {(st?.active || started) && <Button onClick={next}>{t("setup.next")}</Button>}
      </Footer>
    </>
  );
}

// 4. Free episodes --------------------------------------------------------

export function EpisodesStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const episodes = useFreeEpisodes();
  const games = useGames();
  const jobs = useJobs().data ?? [];
  const jobLabel = useJobLabel();
  const start = useStartJob((gameId: string) => commands.installFreeEpisode(gameId));
  // `null` until the user changes something: everything not yet installed.
  const [picked, setPicked] = useState<Set<string> | null>(null);

  const available = (episodes.data ?? []).filter((e) => !e.installed);
  const selected = picked ?? new Set(available.map((e) => e.gameId));
  const jobFor = (e: FreeEpisode) =>
    jobs.find((j) => j.kind === "shareware" && j.subject === e.gameId);
  const toStart = available.filter((e) => selected.has(e.gameId) && !jobFor(e));

  function toggle(id: string) {
    const set = new Set(selected);
    if (set.has(id)) set.delete(id);
    else set.add(id);
    setPicked(set);
  }

  async function download() {
    for (const e of toStart) await start.mutateAsync(e.gameId);
    next();
  }

  return (
    <>
      <StepHeader title={t("setup.episodes.title")}>
        <p>{t("setup.episodes.intro")}</p>
      </StepHeader>
      <OfflineNotice />
      <ul className="grid gap-3">
        {episodes.data?.map((e) => {
          const game = games.data?.find((g) => g.id === e.gameId);
          const version = game?.versions.find((v) => v.id === e.versionId);
          const job = jobFor(e);
          return (
            <li key={e.gameId} className="grid gap-2 rounded-lg border border-border p-3">
              <label className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--bb-accent)]"
                  checked={e.installed || selected.has(e.gameId)}
                  disabled={e.installed || !!job}
                  onChange={() => toggle(e.gameId)}
                />
                <span className="font-semibold">{gameTitle(games.data, e.gameId)}</span>
                {version && <Badge>{version.label}</Badge>}
                <span className="ml-auto text-xs text-muted">
                  {e.installed
                    ? t("setup.episodes.installed")
                    : t("setup.episodes.size", { size: formatMegabytes(e.size) })}
                </span>
              </label>
              {job && <JobRow job={job} label={jobLabel(job)} />}
            </li>
          );
        })}
      </ul>
      <p className="text-xs text-muted">{t("setup.episodes.terms")}</p>
      {episodes.error && <Notice tone="error">{errorMessage(t, episodes.error)}</Notice>}
      {start.error && <Notice tone="error">{errorMessage(t, start.error)}</Notice>}
      <Footer back={back} skip={toStart.length > 0 ? next : undefined}>
        {toStart.length > 0 ? (
          <Button onClick={download} disabled={start.isPending}>
            {t("setup.episodes.download", { count: toStart.length })}
          </Button>
        ) : (
          <Button onClick={next}>{t("setup.next")}</Button>
        )}
      </Footer>
    </>
  );
}

// 5. Already own Keen? ----------------------------------------------------

const scanKey = ["storeScan"] as const;

export function OwnedStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const games = useGames();
  const scan = useQuery({ queryKey: scanKey, queryFn: () => unwrap(commands.scanStores()) });
  const [picked, setPicked] = useState<Set<string> | null>(null);

  const key = (install: StoreInstall, gameId: string) => `${install.path}\n${gameId}`;
  const importable = (scan.data ?? []).flatMap((i) =>
    i.games.filter((g) => !g.imported).map((g) => key(i, g.gameId)),
  );
  const selected = picked ?? new Set(importable);
  const count = importable.filter((k) => selected.has(k)).length;

  const refresh = () =>
    Promise.all([
      client.invalidateQueries({ queryKey: profilesKey }),
      client.invalidateQueries({ queryKey: setupStatusKey }),
      client.invalidateQueries({ queryKey: scanKey }),
    ]);

  const importStores = useMutation({
    mutationFn: async () => {
      for (const install of scan.data ?? []) {
        const ids = install.games
          .filter((g) => !g.imported && selected.has(key(install, g.gameId)))
          .map((g) => g.gameId);
        if (ids.length > 0)
          await unwrap(commands.importStoreGames(install.store, install.path, ids));
      }
    },
    onSuccess: async () => {
      await refresh();
      next();
    },
  });

  const importOther = useMutation({
    mutationFn: (path: string) => unwrap(commands.importGames(path)),
    onSuccess: refresh,
  });

  async function pick(kind: "folder" | "zip") {
    const path = await open(
      kind === "folder" ? { directory: true } : { filters: [{ name: "ZIP", extensions: ["zip"] }] },
    );
    if (path) importOther.mutate(path);
  }

  function toggle(k: string) {
    const set = new Set(selected);
    if (set.has(k)) set.delete(k);
    else set.add(k);
    setPicked(set);
  }

  const error = scan.error ?? importStores.error ?? importOther.error;
  return (
    <>
      <StepHeader title={t("setup.owned.title")}>
        <p>{t("setup.owned.intro")}</p>
      </StepHeader>

      {scan.isPending && <Notice>{t("setup.owned.scanning")}</Notice>}
      {scan.data?.length === 0 && <p className="text-sm">{t("setup.owned.noneFound")}</p>}
      <ul className="grid gap-3">
        {scan.data?.map((install) => (
          <li key={install.path} className="grid gap-2 rounded-lg border border-border p-3">
            <p className="flex flex-wrap items-center gap-2 text-sm">
              <Badge>{t(`setup.owned.store.${install.store}`)}</Badge>
              <span className="text-xs break-all text-muted">{install.path}</span>
            </p>
            {install.games.map((g) => (
              <label key={g.gameId} className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--bb-accent)]"
                  checked={g.imported || selected.has(key(install, g.gameId))}
                  disabled={g.imported}
                  onChange={() => toggle(key(install, g.gameId))}
                />
                <span>{gameTitle(games.data, g.gameId)}</span>
                {g.imported && (
                  <span className="ml-auto text-xs text-muted">{t("setup.owned.imported")}</span>
                )}
              </label>
            ))}
          </li>
        ))}
      </ul>

      <div className="grid gap-3">
        <p className="text-sm text-muted">{t("setup.owned.other")}</p>
        <div className="flex flex-wrap gap-3">
          <Button
            variant="secondary"
            onClick={() => pick("folder")}
            disabled={importOther.isPending}
          >
            <FolderOpen className="size-4" aria-hidden />
            {t("library.importFolder")}
          </Button>
          <Button variant="secondary" onClick={() => pick("zip")} disabled={importOther.isPending}>
            <FileArchive className="size-4" aria-hidden />
            {t("library.importZip")}
          </Button>
        </div>
        {importOther.isPending && <Notice>{t("library.importing")}</Notice>}
        {importOther.isSuccess && (
          <Notice>{t("library.imported", { count: importOther.data.length })}</Notice>
        )}
      </div>

      <div className="grid gap-1 text-sm">
        <p className="text-muted">{t("setup.owned.buy")}</p>
        <p className="flex flex-wrap gap-4">
          {(["gog", "steam"] as const).map((store) => (
            <button
              key={store}
              type="button"
              className="text-ink underline underline-offset-2"
              onClick={() => openUrl(STORE_LINKS[store])}
            >
              {t(`setup.owned.store.${store}`)}
            </button>
          ))}
        </p>
      </div>

      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
      <Footer back={back} skip={count > 0 ? next : undefined}>
        {count > 0 ? (
          <Button onClick={() => importStores.mutate()} disabled={importStores.isPending}>
            {t("setup.owned.import", { count })}
          </Button>
        ) : (
          <Button onClick={next}>{t("setup.next")}</Button>
        )}
      </Footer>
    </>
  );
}

// 7. Done -----------------------------------------------------------------

export function DoneStep({ back, finish, finishing }: StepProps) {
  const { t } = useTranslation();
  const jobs = useJobs().data ?? [];
  const running = jobs.some((j) => !isFinished(j));
  return (
    <>
      <StepHeader title={t("setup.done.title")}>
        <p>{running ? t("setup.done.running") : t("setup.done.ready")}</p>
      </StepHeader>
      {jobs.length > 0 ? (
        <JobList jobs={jobs} />
      ) : (
        <PartyPopper className="size-10 text-accent" aria-hidden />
      )}
      <Footer back={back}>
        <Button onClick={finish} disabled={finishing}>
          {t("setup.done.open")}
        </Button>
      </Footer>
    </>
  );
}
