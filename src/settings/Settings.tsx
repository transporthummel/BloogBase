import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Link, useNavigate } from "@tanstack/react-router";
import { Eraser, FileCog, Keyboard, RefreshCw, Wand } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type BackupSettings, type DisplayOptions, type WhenInactive } from "../bindings";
import { ConfigEditor } from "../config/LazyConfigEditor";
import { oneButtonFiringKey } from "../profile/firing";
import { configKey } from "../config/queries";
import { errorMessage } from "../errors";
import { useAppUpdate, useCheckForUpdate } from "../updates/queries";
import { UpdateCard } from "../updates/UpdateCard";
import { unwrap } from "../ipc";
import { setupStatusKey } from "../setup/queries";
import {
  getShowTrivia,
  getTheme,
  setLanguage,
  setShowTrivia,
  setTheme,
  type Language,
  type Theme,
} from "../preferences";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { OldRuntimeWarning } from "../runtime/OldRuntimeWarning";
import { useRuntimeMutation, useRuntimeStatus } from "../runtime/queries";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";
import { ScoreBoxField, SoundSection } from "./GameSettings";
import { KeysDialog } from "./KeysDialog";
import { Field, Group, selectClass } from "./parts";
import { useGameSettings } from "./queries";

/** The cards of the page, in order, for the list that jumps to them. */
const sectionIds = ["appearance", "pictureSound", "controls", "saves", "dosbox", "app"] as const;
type SectionId = (typeof sectionIds)[number];

export function Settings() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const [theme, setThemeState] = useState(getTheme);
  const [showTrivia, setShowTriviaState] = useState(getShowTrivia);
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });
  const titles: Record<SectionId, string> = {
    appearance: t("settings.appearance"),
    pictureSound: t("settings.pictureSound"),
    controls: t("settings.controls"),
    saves: t("settings.backups"),
    dosbox: t("runtime.title"),
    app: t("settings.app"),
  };

  return (
    <>
      <PageHeader title={t("settings.title")} />
      <div className="grid gap-6 lg:grid-cols-[minmax(0,42rem)_11rem] lg:items-start">
        <SectionNav titles={titles} />
        <div className="grid min-w-0 gap-4 lg:order-first">
          <Section id="appearance" title={titles.appearance}>
            <Field label={t("settings.language")}>
              <select
                className={`${selectClass} w-44`}
                value={i18n.language}
                onChange={(e) => {
                  const lng = e.target.value as Language;
                  setLanguage(lng);
                  i18n.changeLanguage(lng);
                }}
              >
                <option value="en">English</option>
                <option value="de">Deutsch</option>
              </select>
            </Field>
            <Field label={t("settings.theme")}>
              <select
                className={`${selectClass} w-44`}
                value={theme}
                onChange={(e) => {
                  const value = e.target.value as Theme;
                  setTheme(value);
                  setThemeState(value);
                }}
              >
                <option value="dark">{t("settings.themeDark")}</option>
                <option value="light">{t("settings.themeLight")}</option>
                <option value="system">{t("settings.themeSystem")}</option>
              </select>
            </Field>
            <div className="grid gap-1">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={showTrivia}
                  onChange={(e) => {
                    setShowTrivia(e.target.checked);
                    setShowTriviaState(e.target.checked);
                  }}
                />
                {t("settings.showTrivia")}
              </label>
              <p className="ml-6 max-w-prose text-xs text-muted">{t("settings.showTriviaHint")}</p>
            </div>
            <StartFullscreenOption />
          </Section>
          <Section id="pictureSound" title={titles.pictureSound}>
            <DisplaySection>
              <ScoreBoxField />
            </DisplaySection>
            <SoundSection />
            <InactiveSection />
            <LegacyPcSection />
          </Section>
          <Section id="controls" title={titles.controls}>
            <ControlsSection />
          </Section>
          <Section id="saves" title={titles.saves}>
            <BackupSection />
          </Section>
          <Section id="dosbox" title={titles.dosbox}>
            <RuntimeSection />
            <GlobalConfigSection />
          </Section>
          <Section id="app" title={titles.app}>
            <UpdateSection />
            <KeenwikiCacheSection />
            <Group title={t("settings.setup")}>
              <p className="text-sm text-muted">{t("settings.rerunHint")}</p>
              <div>
                <Button
                  variant="secondary"
                  onClick={async () => {
                    await commands.setSetupStep("welcome");
                    await queryClient.invalidateQueries({ queryKey: setupStatusKey });
                    navigate({ to: "/setup" });
                  }}
                >
                  <Wand className="size-4" aria-hidden />
                  {t("settings.rerunSetup")}
                </Button>
              </div>
            </Group>
            <Group title={t("settings.about")}>
              <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
                <dt className="text-muted">{t("settings.version")}</dt>
                <dd className="font-pixel text-xs leading-5">{info.data?.version}</dd>
                <dt className="text-muted">{t("settings.dataDir")}</dt>
                <dd className="break-all select-text">{info.data?.dataDir}</dd>
              </dl>
              <p className="text-sm">
                <Link to="/about" className="underline underline-offset-2">
                  {t("settings.aboutMore")}
                </Link>
              </p>
            </Group>
          </Section>
        </div>
      </div>
    </>
  );
}

/**
 * Jumps to the cards: beside them from 1024px, where it stays in view and
 * marks the card being read, above them on narrower windows.
 */
function SectionNav({ titles }: { titles: Record<SectionId, string> }) {
  const { t } = useTranslation();
  const nav = useRef<HTMLElement>(null);
  const [current, setCurrent] = useState<SectionId>(sectionIds[0]);

  useEffect(() => {
    const main = nav.current?.closest("main");
    if (!main) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const top = main.getBoundingClientRect().top;
      let at: SectionId = sectionIds[0];
      for (const id of sectionIds) {
        const el = document.getElementById(`settings-${id}`);
        if (el && el.getBoundingClientRect().top - top <= 96) at = id;
      }
      // At the bottom the last card can't reach the top: it counts anyway.
      const atEnd =
        main.scrollTop > 0 && main.scrollTop + main.clientHeight >= main.scrollHeight - 2;
      setCurrent(atEnd ? sectionIds[sectionIds.length - 1] : at);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    main.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      main.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    };
  }, []);

  function jump(id: SectionId, byKeyboard: boolean) {
    const section = document.getElementById(`settings-${id}`);
    if (!section) return;
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    section.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "start" });
    // Keyboard users go on reading from there.
    if (byKeyboard) section.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
  }

  return (
    <nav ref={nav} aria-label={t("settings.sections")} className="lg:sticky lg:top-6">
      <ul className="flex flex-wrap gap-1 lg:grid lg:gap-0.5">
        {sectionIds.map((id) => (
          <li key={id}>
            <button
              type="button"
              onClick={(e) => jump(id, e.detail === 0)}
              aria-current={current === id ? "location" : undefined}
              className={`flex h-9 w-full items-center rounded-lg px-3 text-left text-sm transition hover:bg-surface-raised hover:text-ink ${
                current === id ? "bg-surface-raised text-ink" : "text-muted"
              }`}
            >
              {titles[id]}
            </button>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function UpdateSection() {
  const { t } = useTranslation();
  const update = useAppUpdate({ enabled: false });
  const checkNow = useCheckForUpdate();
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });
  return (
    <Group title={t("updates.title")}>
      {info.data?.updatesByPackageManager ? (
        <p className="text-sm">{t("updates.byPackageManager")}</p>
      ) : update.data ? (
        <UpdateCard update={update.data} />
      ) : (
        <>
          {checkNow.isSuccess && <Notice>{t("updates.upToDate")}</Notice>}
          {checkNow.isError && (
            <Notice tone="error">
              {t("updates.checkFailed", { message: String(checkNow.error) })}
            </Notice>
          )}
          <div>
            <Button
              variant="secondary"
              onClick={() => checkNow.mutate()}
              disabled={checkNow.isPending}
            >
              <RefreshCw className="size-4" aria-hidden />
              {t("updates.check")}
            </Button>
          </div>
        </>
      )}
    </Group>
  );
}

function RuntimeSection() {
  const { t } = useTranslation();
  const status = useRuntimeStatus();
  const setCustom = useRuntimeMutation((path: string | null) =>
    unwrap(commands.setCustomRuntime(path)),
  );
  const st = status.data;
  const active = st?.active;
  const managedOutdated = st?.managed && st.managed.version !== st.pinnedVersion;

  async function chooseCustom() {
    const path = await open({ title: t("runtime.chooseCustom") });
    if (path) setCustom.mutate(path);
  }

  return (
    <Group title={t("settings.dosbox")}>
      <p className="text-sm">
        {!st
          ? null
          : !active
            ? t("runtime.notInstalled")
            : active.managed
              ? t("runtime.activeManaged", { version: active.version })
              : t("runtime.activeCustom", { version: active.version })}
      </p>
      {active && <p className="text-xs break-all text-muted select-text">{active.path}</p>}
      {st?.crtSwitchIgnored && <OldRuntimeWarning testedVersion={st.pinnedVersion} />}

      {/* With the player's own DOSBox in use, ours is downloaded after
          switching back to it, not next to it. */}
      {st?.downloadAvailable && (!st.managed || managedOutdated) && (!active || active.managed) && (
        <InstallRuntimeButton
          label={
            st.managed
              ? t("runtime.update", { version: st.pinnedVersion })
              : t("runtime.install", { version: st.pinnedVersion })
          }
        />
      )}
      {st && !st.downloadAvailable && <Notice>{t("errors.unsupportedPlatform")}</Notice>}

      <div className="flex flex-wrap gap-3">
        <Button variant="secondary" onClick={chooseCustom} disabled={setCustom.isPending}>
          <FileCog className="size-4" aria-hidden />
          {t("runtime.chooseCustom")}
        </Button>
        {active && !active.managed && (
          <Button
            variant="ghost"
            onClick={() => setCustom.mutate(null)}
            disabled={setCustom.isPending}
          >
            {t(st?.managed ? "runtime.useManaged" : "runtime.useManagedFirst")}
          </Button>
        )}
      </div>
      <p className="max-w-prose text-xs text-muted">{t("runtime.customHint")}</p>
      {setCustom.error && <Notice tone="error">{errorMessage(t, setCustom.error)}</Notice>}
    </Group>
  );
}

// Under "config" so saving the DOSBox settings refreshes what they override.
const displayOptionsKey = ["config", "display"] as const;

/** The simple DOSBox options for all games, and a way to save them. */
function useDisplayOptions() {
  const queryClient = useQueryClient();
  const view = useQuery({
    queryKey: displayOptionsKey,
    queryFn: () => unwrap(commands.displayOptions()),
  });
  const update = useMutation({
    mutationFn: (o: DisplayOptions) => unwrap(commands.setDisplayOptions(o)),
    // Also the per-game views of what DOSBox gets.
    onSettled: () => queryClient.invalidateQueries({ queryKey: ["config"] }),
  });
  return { view, update };
}

/** Simple DOSBox options for all games, so nobody needs to know DOSBox. */
export function DisplaySection({ children }: { children?: ReactNode }) {
  const { t } = useTranslation();
  const { view, update } = useDisplayOptions();
  const v = view.data;
  const options = ["fullscreen", "crt", "vsync"] as const;

  return (
    <Group title={t("settings.display")}>
      {v &&
        options.map((name) => {
          const overridden = v.overridden.includes(name);
          return (
            <div key={name} className="grid gap-1">
              <label className="flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={v.options[name]}
                  disabled={overridden || update.isPending}
                  onChange={(e) => update.mutate({ ...v.options, [name]: e.target.checked })}
                />
                {t(`settings.displayOption.${name}`)}
              </label>
              <p className="ml-6 max-w-prose text-xs text-muted">
                {overridden ? t("settings.displayOverridden") : t(`settings.displayHint.${name}`)}
              </p>
            </div>
          );
        })}
      {children}
      {v && <p className="max-w-prose text-xs text-muted">{t("settings.displayHint.all")}</p>}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </Group>
  );
}

/** What the game does while another window is active. */
export function InactiveSection() {
  const { t } = useTranslation();
  const { view, update } = useDisplayOptions();
  const v = view.data;
  const overridden = v?.overridden.includes("inactive");
  const choices: WhenInactive[] = ["run", "mute", "pause"];

  return (
    <Group title={t("settings.inactive")}>
      {v && (
        <div className="grid gap-1">
          <Field label={t("settings.inactiveLabel")}>
            <select
              className={`${selectClass} w-52`}
              value={v.options.inactive}
              disabled={overridden || update.isPending}
              onChange={(e) =>
                update.mutate({ ...v.options, inactive: e.target.value as WhenInactive })
              }
            >
              {choices.map((c) => (
                <option key={c} value={c}>
                  {t(`settings.inactiveChoice.${c}`)}
                </option>
              ))}
            </select>
          </Field>
          <p className="max-w-prose text-xs text-muted">
            {overridden ? t("settings.displayOverridden") : t("settings.inactiveHint")}
          </p>
        </div>
      )}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </Group>
  );
}

/** A slow hard disk you can hear, as on a PC of the time. */
export function LegacyPcSection() {
  const { t } = useTranslation();
  const { view, update } = useDisplayOptions();
  const v = view.data;
  const overridden = v?.overridden.includes("legacyPc");

  return (
    <Group title={t("settings.legacyPc")}>
      {v && (
        <div className="grid gap-1">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={v.options.legacyPc}
              disabled={overridden || update.isPending}
              onChange={(e) => update.mutate({ ...v.options, legacyPc: e.target.checked })}
            />
            {t("settings.legacyPcOption")}
          </label>
          <p className="ml-6 max-w-prose text-xs text-muted">
            {overridden ? t("settings.displayOverridden") : t("settings.legacyPcHint")}
          </p>
        </div>
      )}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </Group>
  );
}

const startFullscreenKey = ["startFullscreen"] as const;

/** Whether BloogBase starts in fullscreen; the sidebar switches it anytime. */
function StartFullscreenOption() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const setting = useQuery({
    queryKey: startFullscreenKey,
    queryFn: () => unwrap(commands.startFullscreen()),
  });
  const update = useMutation({
    mutationFn: (on: boolean) => unwrap(commands.setStartFullscreen(on)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: startFullscreenKey }),
  });

  return (
    <div className="grid gap-1">
      {setting.data !== undefined && (
        <>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={setting.data}
              disabled={update.isPending}
              onChange={(e) => update.mutate(e.target.checked)}
            />
            {t("settings.startFullscreen")}
          </label>
          <p className="ml-6 max-w-prose text-xs text-muted">{t("settings.startFullscreenHint")}</p>
        </>
      )}
      {(setting.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, setting.error ?? update.error)}</Notice>
      )}
    </div>
  );
}

function ControlsSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [choosingKeys, setChoosingKeys] = useState(false);
  const { settings: game } = useGameSettings();
  const keysSet = game ? Object.values(game.keys).filter((k) => k !== null).length : 0;
  const firing = useQuery({
    queryKey: oneButtonFiringKey,
    queryFn: () => unwrap(commands.oneButtonFiring()),
  });
  const update = useMutation({
    mutationFn: (on: boolean) => unwrap(commands.setOneButtonFiring(on)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: oneButtonFiringKey }),
  });

  return (
    <>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div className="grid gap-0.5 text-sm">
          <span>{t("settings.keysTitle")}</span>
          <span className="text-xs text-muted">
            {keysSet > 0 ? t("settings.keysSet", { count: keysSet }) : t("settings.keysNone")}
          </span>
        </div>
        <Button variant="secondary" onClick={() => setChoosingKeys(true)}>
          <Keyboard className="size-4" aria-hidden />
          {t("settings.keysOpen")}
        </Button>
      </div>
      {firing.data !== undefined && (
        <div className="grid gap-1">
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={firing.data}
              disabled={update.isPending}
              onChange={(e) => update.mutate(e.target.checked)}
            />
            {t("settings.oneButtonFiring")}
          </label>
          <p className="ml-6 max-w-prose text-xs text-muted">{t("settings.oneButtonFiringHint")}</p>
        </div>
      )}
      {choosingKeys && <KeysDialog onClose={() => setChoosingKeys(false)} />}
      {(firing.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, firing.error ?? update.error)}</Notice>
      )}
    </>
  );
}

const backupSettingsKey = ["backupSettings"] as const;

function BackupSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const settings = useQuery({
    queryKey: backupSettingsKey,
    queryFn: () => unwrap(commands.backupSettings()),
  });
  const update = useMutation({
    mutationFn: (s: BackupSettings) => unwrap(commands.setBackupSettings(s)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: backupSettingsKey }),
  });
  const s = settings.data;

  return (
    <>
      {s && (
        <>
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              className="mt-1"
              checked={s.auto}
              onChange={(e) => update.mutate({ ...s, auto: e.target.checked })}
            />
            <span>
              {t("settings.autoBackup")}
              <span className="block max-w-prose text-xs text-muted">
                {t("settings.autoBackupHint")}
              </span>
            </span>
          </label>
          <Field label={t("settings.keepBackups")}>
            <input
              type="number"
              min={1}
              max={100}
              className={`${selectClass} w-24`}
              defaultValue={s.keep}
              onBlur={(e) => {
                const keep = Math.round(Number(e.target.value));
                if (keep >= 1 && keep <= 100 && keep !== s.keep) update.mutate({ ...s, keep });
              }}
            />
          </Field>
          <p className="max-w-prose text-xs text-muted">{t("settings.backupsHint")}</p>
        </>
      )}
      {(settings.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, settings.error ?? update.error)}</Notice>
      )}
    </>
  );
}

export function KeenwikiCacheSection() {
  const { t } = useTranslation();
  const clear = useMutation({ mutationFn: () => unwrap(commands.clearKeenwikiCache()) });

  return (
    <Group title={t("settings.keenwikiCache")}>
      <p className="text-sm text-muted">{t("settings.keenwikiCacheHint")}</p>
      <div className="flex items-center gap-3">
        <Button variant="secondary" disabled={clear.isPending} onClick={() => clear.mutate()}>
          <Eraser className="size-4" aria-hidden />
          {t("settings.clearKeenwikiCache")}
        </Button>
        {clear.isSuccess && (
          <span role="status" className="text-sm text-muted">
            {t("settings.keenwikiCacheCleared")}
          </span>
        )}
      </div>
      {clear.error && <Notice tone="error">{errorMessage(t, clear.error)}</Notice>}
    </Group>
  );
}

function GlobalConfigSection() {
  const { t } = useTranslation();
  const view = useQuery({
    queryKey: configKey(null),
    queryFn: () => unwrap(commands.configView(null)),
  });
  return (
    <Group title={t("config.globalTitle")}>
      <p className="text-sm text-muted">{t("config.globalHint")}</p>
      {view.data && (
        <ConfigEditor
          profileId={null}
          text={view.data.global}
          warnings={view.data.globalWarnings}
          label={t("config.globalTitle")}
        />
      )}
      {view.error && <Notice tone="error">{errorMessage(t, view.error)}</Notice>}
    </Group>
  );
}

/** A card of related settings, split into groups. */
function Section({ id, title, children }: { id: SectionId; title: string; children: ReactNode }) {
  return (
    <Card id={`settings-${id}`} className="scroll-mt-6 p-5">
      <h2 tabIndex={-1} className="mb-4 font-semibold">
        {title}
      </h2>
      <div className="grid gap-5">{children}</div>
    </Card>
  );
}
