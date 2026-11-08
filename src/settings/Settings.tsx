import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Link, useNavigate } from "@tanstack/react-router";
import { Eraser, FileCog, Keyboard, RefreshCw, Wand } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
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
import { Check, Field, Group, selectClass } from "./parts";
import { useGameSettings } from "./queries";

/** The cards of the page, in order, for the list that jumps to them. */
const sectionIds = [
  "appearance",
  "picture",
  "soundBackground",
  "controls",
  "saves",
  "dosbox",
  "app",
] as const;
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
    picture: t("settings.display"),
    soundBackground: t("settings.soundBackground"),
    controls: t("settings.controls"),
    saves: t("settings.backups"),
    dosbox: t("runtime.title"),
    app: t("settings.app"),
  };

  return (
    <>
      <PageHeader title={t("settings.title")} />
      <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,42rem)_11rem] lg:items-start">
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
            <Check
              label={t("settings.showTrivia")}
              hint={t("settings.showTriviaHint")}
              checked={showTrivia}
              onChange={(e) => {
                setShowTrivia(e.target.checked);
                setShowTriviaState(e.target.checked);
              }}
            />
          </Section>
          <Section id="picture" title={titles.picture} intro={t("settings.nextStart")}>
            <DisplaySection>
              <ScoreBoxField />
            </DisplaySection>
          </Section>
          <Section
            id="soundBackground"
            title={titles.soundBackground}
            intro={t("settings.nextStart")}
          >
            <SoundSection />
            <InactiveSection />
            <LegacyPcSection />
          </Section>
          <Section id="controls" title={titles.controls} intro={t("settings.nextStart")}>
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
  // The card jumped to stays marked, even when a short one can't reach the
  // top, until the player scrolls on their own.
  const jumped = useRef<SectionId | null>(null);

  useEffect(() => {
    const main = nav.current?.closest("main");
    if (!main) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      if (jumped.current) {
        setCurrent(jumped.current);
        return;
      }
      const top = main.getBoundingClientRect().top;
      let at: SectionId = sectionIds[0];
      for (const id of sectionIds) {
        const el = document.getElementById(`settings-${id}`);
        if (el && el.getBoundingClientRect().top - top <= main.clientHeight * 0.3) at = id;
      }
      // At the bottom the last card can't reach the top: it counts anyway.
      const atEnd =
        main.scrollTop > 0 && main.scrollTop + main.clientHeight >= main.scrollHeight - 2;
      setCurrent(atEnd ? sectionIds[sectionIds.length - 1] : at);
    };
    const onScroll = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    const ownScroll = () => {
      jumped.current = null;
    };
    update();
    main.addEventListener("scroll", onScroll, { passive: true });
    main.addEventListener("wheel", ownScroll, { passive: true });
    main.addEventListener("pointerdown", ownScroll);
    main.addEventListener("keydown", ownScroll);
    return () => {
      main.removeEventListener("scroll", onScroll);
      main.removeEventListener("wheel", ownScroll);
      main.removeEventListener("pointerdown", ownScroll);
      main.removeEventListener("keydown", ownScroll);
      cancelAnimationFrame(frame);
    };
  }, []);

  // As a bar on narrow windows the list may not fit: the marked item is
  // scrolled into it, and a fade shows on the side where more items are.
  const list = useRef<HTMLUListElement>(null);
  const [more, setMore] = useState({ before: false, after: false });
  const measure = useCallback(() => {
    const ul = list.current;
    if (!ul) return;
    const before = ul.scrollLeft > 1;
    const after = ul.scrollLeft + ul.clientWidth < ul.scrollWidth - 1;
    setMore((m) => (m.before === before && m.after === after ? m : { before, after }));
  }, []);

  useEffect(() => {
    const ul = list.current;
    const item = ul?.querySelector<HTMLElement>("[aria-current]");
    if (!ul || !item || ul.scrollWidth <= ul.clientWidth) {
      measure();
      return;
    }
    const box = ul.getBoundingClientRect();
    const at = item.getBoundingClientRect();
    if (at.left < box.left) ul.scrollLeft -= box.left - at.left + 24;
    else if (at.right > box.right) ul.scrollLeft += at.right - box.right + 24;
    measure();
  }, [current, measure]);

  useEffect(() => {
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [measure]);

  const fade = [
    more.before ? "transparent, black 1.5rem" : "black",
    more.after ? "black calc(100% - 1.5rem), transparent" : "black",
  ].join(", ");

  function jump(id: SectionId, byKeyboard: boolean) {
    const section = document.getElementById(`settings-${id}`);
    if (!section) return;
    jumped.current = id;
    setCurrent(id);
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    section.scrollIntoView({ behavior: still ? "auto" : "smooth", block: "start" });
    // Keyboard users go on reading from there.
    if (byKeyboard) section.querySelector<HTMLElement>("h2")?.focus({ preventScroll: true });
  }

  return (
    <nav
      ref={nav}
      aria-label={t("settings.sections")}
      className="sticky top-0 z-10 -mx-6 border-b border-border bg-bg px-6 py-2 lg:top-6 lg:mx-0 lg:border-0 lg:bg-transparent lg:p-0"
    >
      <ul
        ref={list}
        onScroll={measure}
        style={{ maskImage: `linear-gradient(to right, ${fade})` }}
        className="flex gap-0.5 overflow-x-auto [scrollbar-width:none] min-[900px]:gap-1 lg:grid lg:gap-0.5 lg:overflow-visible"
      >
        {sectionIds.map((id) => (
          <li key={id}>
            <button
              type="button"
              onClick={(e) => jump(id, e.detail === 0)}
              aria-current={current === id ? "location" : undefined}
              className={`flex h-9 w-full items-center rounded-lg px-2 text-left text-sm whitespace-nowrap min-[900px]:px-2.5 lg:px-3 transition hover:bg-surface-raised hover:text-ink ${
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
  const options = ["crt", "vsync"] as const;

  return (
    <>
      <FullscreenField />
      {v &&
        options.map((name) => {
          const overridden = v.overridden.includes(name);
          return (
            <Check
              key={name}
              label={t(`settings.displayOption.${name}`)}
              hint={
                overridden ? t("settings.displayOverridden") : t(`settings.displayHint.${name}`)
              }
              checked={v.options[name]}
              disabled={overridden || update.isPending}
              onChange={(e) => update.mutate({ ...v.options, [name]: e.target.checked })}
            />
          );
        })}
      {children}
      {v && <p className="max-w-prose text-xs text-muted">{t("settings.displayHint.all")}</p>}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </>
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
        <Field
          label={t("settings.inactiveLabel")}
          hint={overridden ? t("settings.displayOverridden") : t("settings.inactiveHint")}
        >
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
        <Check
          label={t("settings.legacyPcOption")}
          hint={overridden ? t("settings.displayOverridden") : t("settings.legacyPcHint")}
          checked={v.options.legacyPc}
          disabled={overridden || update.isPending}
          onChange={(e) => update.mutate({ ...v.options, legacyPc: e.target.checked })}
        />
      )}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </Group>
  );
}

const startFullscreenKey = ["startFullscreen"] as const;

type FullscreenChoice = "none" | "games" | "both";

/**
 * One choice for two settings: whether games start in fullscreen (the display
 * option) and whether BloogBase does, which takes its games along.
 */
function FullscreenField() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { view, update } = useDisplayOptions();
  const app = useQuery({
    queryKey: startFullscreenKey,
    queryFn: () => unwrap(commands.startFullscreen()),
  });
  const updateApp = useMutation({
    mutationFn: (on: boolean) => unwrap(commands.setStartFullscreen(on)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: startFullscreenKey }),
  });
  const v = view.data;
  const error = app.error ?? updateApp.error;

  if (!v || app.data === undefined) {
    return error ? <Notice tone="error">{errorMessage(t, error)}</Notice> : null;
  }
  // DOSBox settings for all games may set it; BloogBase in fullscreen still
  // takes the games along.
  const overridden = v.overridden.includes("fullscreen");
  const value: FullscreenChoice = app.data ? "both" : v.options.fullscreen ? "games" : "none";

  function choose(choice: FullscreenChoice) {
    const games = choice !== "none";
    if (!overridden && v && games !== v.options.fullscreen) {
      update.mutate({ ...v.options, fullscreen: games });
    }
    if ((choice === "both") !== app.data) updateApp.mutate(choice === "both");
  }

  return (
    <div className="grid gap-1">
      <Field
        label={t("settings.fullscreen")}
        hint={overridden ? t("settings.fullscreenOverridden") : t("settings.fullscreenHint")}
      >
        <select
          className={`${selectClass} w-52`}
          value={value}
          disabled={update.isPending || updateApp.isPending}
          onChange={(e) => choose(e.target.value as FullscreenChoice)}
        >
          {(["none", "games", "both"] as const).map((c) => (
            <option key={c} value={c}>
              {t(`settings.fullscreenChoice.${c}`)}
            </option>
          ))}
        </select>
      </Field>
      {error && <Notice tone="error">{errorMessage(t, error)}</Notice>}
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
        <Check
          label={t("settings.oneButtonFiring")}
          hint={t("settings.oneButtonFiringHint")}
          checked={firing.data}
          disabled={update.isPending}
          onChange={(e) => update.mutate(e.target.checked)}
        />
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
          <Check
            label={t("settings.autoBackup")}
            hint={t("settings.autoBackupHint")}
            checked={s.auto}
            onChange={(e) => update.mutate({ ...s, auto: e.target.checked })}
          />
          <Field label={t("settings.keepBackups")} hint={t("settings.backupsHint")}>
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
function Section({
  id,
  title,
  intro,
  children,
}: {
  id: SectionId;
  title: string;
  /** Said once for the whole card instead of under each setting. */
  intro?: string;
  children: ReactNode;
}) {
  return (
    <Card id={`settings-${id}`} className="scroll-mt-20 p-5 lg:scroll-mt-6">
      {/* Focused after a jump from the keyboard; the app's ring shows where. */}
      <h2
        tabIndex={-1}
        className={`rounded-sm font-semibold focus-visible:outline-3 focus-visible:outline-offset-2 focus-visible:outline-focus ${intro ? "mb-1" : "mb-4"}`}
      >
        {title}
      </h2>
      {intro && <p className="mb-5 text-sm text-muted">{intro}</p>}
      <div className="grid gap-5">{children}</div>
    </Card>
  );
}
