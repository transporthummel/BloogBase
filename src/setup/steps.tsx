import { useMutation, useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { CircleCheck, FileArchive, FileCog, FolderOpen, PartyPopper } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import logo from "../assets/logo.jpg";
import { commands, type FreeGame, type Install } from "../bindings";
import { gameTitle, useGames } from "../catalog";
import { errorMessage } from "../errors";
import { formatMegabytes } from "../format";
import { unwrap } from "../ipc";
import { JobList, JobRow } from "../jobs/JobList";
import { useJobLabel } from "../jobs/label";
import { isFinished, useJobs, useStartJob } from "../jobs/queries";
import { compare, fitting, type Owned } from "../discover/fit";
import { IndexError } from "../discover/IndexError";
import { useFreeGames, useIndexMods, useIndexStatus } from "../discover/queries";
import { installsKey } from "../library/queries";
import { setLanguage, type Language } from "../preferences";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { useRuntimeMutation, useRuntimeStatus } from "../runtime/queries";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";
import { Notice } from "../ui/Notice";
import { useSetupStatus } from "./queries";
import { StoreLinks, StoreScanList } from "./StoreScanList";
import { useStoreScan } from "./storeScan";
import { usePageTitle } from "../ui/usePageTitle";

export type StepProps = {
  next: () => void;
  back?: () => void;
  finish: () => void;
  finishing: boolean;
};

function StepHeader({ title, children }: { title: string; children?: ReactNode }) {
  usePageTitle(title);
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
  start,
  children,
}: {
  back?: () => void;
  skip?: () => void;
  /** Shown at the left instead of "Back". */
  start?: ReactNode;
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
      {start}
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
      {/* In short windows a wider strip, cut to keep the lettering, so Let's go stays in view at 800×600. */}
      <img
        src={logo}
        alt=""
        className="aspect-[16/9] w-full rounded-xl border border-border object-cover shadow-card [@media(max-height:720px)]:aspect-[3/1] [@media(max-height:720px)]:object-bottom"
      />
      <StepHeader title={t("setup.welcome.title")}>
        <p>{t("setup.welcome.intro")}</p>
        <p>{t("setup.welcome.steps")}</p>
      </StepHeader>
      {/* In the footer, so the first page fits the default window. */}
      <Footer
        start={
          <div role="group" aria-label={t("settings.language")} className="flex flex-wrap gap-1">
            {languages.map(([lng, name]) => (
              <Button
                key={lng}
                variant={i18n.language === lng ? "secondary" : "ghost"}
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
        }
      >
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
      <p className="rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm break-all select-text">
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

// 4. Already own Keen? ----------------------------------------------------

export function OwnedStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const store = useStoreScan(() => next());
  const { count, importStores, refresh } = store;

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

  const error = store.scan.error ?? importStores.error ?? importOther.error;
  return (
    <>
      <StepHeader title={t("setup.owned.title")}>
        <p>{t("setup.owned.intro")}</p>
      </StepHeader>

      <StoreScanList store={store} />

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

      <StoreLinks />

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

// 5. Free episodes --------------------------------------------------------

export function EpisodesStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const free = useFreeGames();
  const games = useGames();
  const installs = useQuery({
    queryKey: installsKey,
    queryFn: () => unwrap(commands.listInstalls()),
  });
  const jobs = useJobs().data ?? [];
  const jobLabel = useJobLabel();
  const start = useStartJob((versionId: string) => commands.installFreeGame(versionId));
  // `null` until the user changes something: everything not yet installed.
  const [picked, setPicked] = useState<Set<string> | null>(null);

  const episodes = free.data?.filter((e) => e.setup);
  const available = (episodes ?? []).filter((e) => !e.installed);
  // A full version added from Steam or GOG makes the shareware redundant;
  // a beta or demo doesn't.
  const isFull = (i: Install) =>
    (games.data?.find((g) => g.id === i.gameId)?.versions.find((v) => v.id === i.versionId)?.kind ??
      "release") === "release";
  const owns = (e: FreeGame) => installs.data?.some((i) => i.gameId === e.gameId && isFull(i));
  const selected = picked ?? new Set(available.filter((e) => !owns(e)).map((e) => e.versionId));
  const jobFor = (e: FreeGame) =>
    jobs.find((j) => j.kind === "freeGame" && j.subject === e.versionId);
  const toStart = available.filter((e) => selected.has(e.versionId) && !jobFor(e));

  function toggle(id: string) {
    const set = new Set(selected);
    if (set.has(id)) set.delete(id);
    else set.add(id);
    setPicked(set);
  }

  async function download() {
    for (const e of toStart) await start.mutateAsync(e.versionId);
    next();
  }

  return (
    <>
      <StepHeader title={t("setup.episodes.title")}>
        <p>{t("setup.episodes.intro")}</p>
      </StepHeader>
      <OfflineNotice />
      <ul className="grid gap-3">
        {episodes?.map((e) => {
          const game = games.data?.find((g) => g.id === e.gameId);
          const version = game?.versions.find((v) => v.id === e.versionId);
          const job = jobFor(e);
          return (
            <li key={e.versionId} className="grid gap-2 rounded-lg border border-border p-3">
              <label className="flex items-center gap-3 text-sm">
                <input
                  type="checkbox"
                  className="size-4 accent-[var(--bb-accent)]"
                  checked={e.installed || selected.has(e.versionId)}
                  disabled={e.installed || !!job}
                  onChange={() => toggle(e.versionId)}
                />
                <span className="font-semibold">{gameTitle(games.data, e.gameId)}</span>
                {version && <Badge>{version.label}</Badge>}
                <span className="ml-auto text-xs text-muted">
                  {e.installed
                    ? t("setup.episodes.installed")
                    : owns(e)
                      ? t("setup.episodes.owned")
                      : t("setup.episodes.size", { size: formatMegabytes(e.size) })}
                </span>
              </label>
              {job && <JobRow job={job} label={jobLabel(job)} />}
            </li>
          );
        })}
      </ul>
      <p className="text-xs text-muted">{t("setup.episodes.terms")}</p>
      {free.error && <Notice tone="error">{errorMessage(t, free.error)}</Notice>}
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

// 6. Try a mod -----------------------------------------------------------

/** Featured index mods that work with what the user has, or will have once
 * the free episodes are downloaded. */
export function ModsStep({ next, back }: StepProps) {
  const { t } = useTranslation();
  const games = useGames().data ?? [];
  const status = useIndexStatus().data;
  const mods = useIndexMods();
  const installs = useQuery({
    queryKey: installsKey,
    queryFn: () => unwrap(commands.listInstalls()),
  });
  const free = useFreeGames();
  const jobs = useJobs().data ?? [];
  const start = useStartJob((indexId: string) =>
    commands.installIndexMod(indexId, null, false, null),
  );
  const [picked, setPicked] = useState<Set<string>>(new Set());

  const coming: Owned[] = (free.data ?? [])
    .filter((e) =>
      jobs.some((j) => j.kind === "freeGame" && j.subject === e.versionId && j.phase !== "failed"),
    )
    .map((e) => ({ gameId: e.gameId, versionId: e.versionId }));
  const owned: Owned[] = [...(installs.data ?? []), ...coming];
  const queued = (id: string) => jobs.some((j) => j.kind === "mod" && j.subject === id);
  const suggestions = (mods.data ?? [])
    .filter((m) => m.entry.featured && !m.modId && fitting(m.entry, owned, games).length > 0)
    .sort((a, b) => compare("featured")(a.entry, b.entry))
    .slice(0, 6);
  const toStart = suggestions.filter((m) => picked.has(m.entry.id) && !queued(m.entry.id));
  const loading = status && status.checkedAt === null && !status.error && status.modCount === 0;

  function toggle(id: string) {
    const set = new Set(picked);
    if (set.has(id)) set.delete(id);
    else set.add(id);
    setPicked(set);
  }

  async function install() {
    for (const m of toStart) await start.mutateAsync(m.entry.id);
    next();
  }

  return (
    <>
      <StepHeader title={t("setup.mods.title")}>
        <p>{t("setup.mods.intro")}</p>
      </StepHeader>
      {loading && <Notice>{t("setup.mods.loading")}</Notice>}
      {!loading &&
        suggestions.length === 0 &&
        (status?.error ? (
          <IndexError error={status.error} cached={status.modCount > 0} wizard />
        ) : (
          <p className="text-sm">{t("setup.mods.none")}</p>
        ))}
      <ul className="grid gap-3">
        {suggestions.map(({ entry }) => (
          <li key={entry.id} className="rounded-lg border border-border p-3">
            <label className="flex items-start gap-3 text-sm">
              <input
                type="checkbox"
                className="mt-0.5 size-4 accent-[var(--bb-accent)]"
                checked={queued(entry.id) || picked.has(entry.id)}
                disabled={queued(entry.id)}
                onChange={() => toggle(entry.id)}
              />
              <span className="grid gap-1">
                <span className="flex flex-wrap items-center gap-2">
                  <span className="font-semibold">{entry.name}</span>
                  <Badge>{t(`discover.kind.${entry.type}`)}</Badge>
                </span>
                <span className="text-xs text-muted">
                  {gameTitle(games, entry.game)}
                  {entry.authors.length > 0 &&
                    ` · ${t("mods.by", { authors: entry.authors.join(", ") })}`}
                </span>
                {entry.description && <span className="text-muted">{entry.description}</span>}
              </span>
            </label>
          </li>
        ))}
      </ul>
      {suggestions.length > 0 && <p className="text-xs text-muted">{t("setup.mods.later")}</p>}
      {start.error && <Notice tone="error">{errorMessage(t, start.error)}</Notice>}
      <Footer back={back} skip={toStart.length > 0 ? next : undefined}>
        {toStart.length > 0 ? (
          <Button onClick={install} disabled={start.isPending}>
            {t("setup.mods.install", { count: toStart.length })}
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
