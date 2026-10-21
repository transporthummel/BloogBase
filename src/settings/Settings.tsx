import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Link, useNavigate } from "@tanstack/react-router";
import { FileCog, RefreshCw, Wand } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands, type BackupSettings, type DisplayOptions } from "../bindings";
import { ConfigEditor } from "../config/ConfigEditor";
import { oneButtonFiringKey } from "../profile/firing";
import { configKey } from "../config/queries";
import { errorMessage } from "../errors";
import { useAppUpdate, useCheckForUpdate } from "../updates/queries";
import { UpdateCard } from "../updates/UpdateCard";
import { unwrap } from "../ipc";
import {
  getShowTrivia,
  getTheme,
  setLanguage,
  setSetupStep,
  setShowTrivia,
  setTheme,
  type Language,
  type Theme,
} from "../preferences";
import { InstallRuntimeButton } from "../runtime/InstallRuntimeButton";
import { useRuntimeMutation, useRuntimeStatus } from "../runtime/queries";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Notice } from "../ui/Notice";
import { PageHeader } from "../ui/PageHeader";

export function Settings() {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [theme, setThemeState] = useState(getTheme);
  const [showTrivia, setShowTriviaState] = useState(getShowTrivia);
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });

  return (
    <>
      <PageHeader title={t("settings.title")} />
      <div className="grid max-w-2xl gap-4">
        <Section title={t("settings.appearance")}>
          <Field label={t("settings.language")}>
            <select
              className={selectClass}
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
              className={selectClass}
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
            <p className="ml-6 text-xs text-muted">{t("settings.showTriviaHint")}</p>
          </div>
        </Section>
        <RuntimeSection />
        <DisplaySection />
        <ControlsSection />
        <BackupSection />
        <GlobalConfigSection />
        <Section title={t("settings.setup")}>
          <p className="text-sm text-muted">{t("settings.rerunHint")}</p>
          <div>
            <Button
              variant="secondary"
              onClick={() => {
                setSetupStep("welcome");
                navigate({ to: "/setup" });
              }}
            >
              <Wand className="size-4" aria-hidden />
              {t("settings.rerunSetup")}
            </Button>
          </div>
        </Section>
        <UpdateSection />
        <Section title={t("settings.about")}>
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
            <dt className="text-muted">{t("settings.version")}</dt>
            <dd className="font-pixel text-xs leading-5">{info.data?.version}</dd>
            <dt className="text-muted">{t("settings.dataDir")}</dt>
            <dd className="break-all">{info.data?.dataDir}</dd>
          </dl>
          <p className="text-sm">
            <Link to="/about" className="underline underline-offset-2">
              {t("settings.aboutMore")}
            </Link>
          </p>
        </Section>
      </div>
    </>
  );
}

function UpdateSection() {
  const { t } = useTranslation();
  const update = useAppUpdate({ enabled: false });
  const checkNow = useCheckForUpdate();
  return (
    <Section title={t("updates.title")}>
      {update.data ? (
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
    </Section>
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
    <Section title={t("runtime.title")}>
      <p className="text-sm">
        {!st
          ? null
          : !active
            ? t("runtime.notInstalled")
            : active.managed
              ? t("runtime.activeManaged", { version: active.version })
              : t("runtime.activeCustom", { version: active.version })}
      </p>
      {active && <p className="text-xs break-all text-muted">{active.path}</p>}

      {st?.downloadAvailable && (!st.managed || managedOutdated) && (
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
            {t("runtime.useManaged")}
          </Button>
        )}
      </div>
      <p className="text-xs text-muted">{t("runtime.customHint")}</p>
      {setCustom.error && <Notice tone="error">{errorMessage(t, setCustom.error)}</Notice>}
    </Section>
  );
}

// Under "config" so saving the DOSBox settings refreshes what they override.
const displayOptionsKey = ["config", "display"] as const;

/** Simple DOSBox options for all games, so nobody needs to know DOSBox. */
export function DisplaySection() {
  const { t } = useTranslation();
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
  const v = view.data;
  const options: (keyof DisplayOptions)[] = ["fullscreen", "crt", "vsync"];

  return (
    <Section title={t("settings.display")}>
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
              <p className="ml-6 text-xs text-muted">
                {overridden ? t("settings.displayOverridden") : t(`settings.displayHint.${name}`)}
              </p>
            </div>
          );
        })}
      {v && <p className="text-xs text-muted">{t("settings.displayHint.all")}</p>}
      {(view.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, view.error ?? update.error)}</Notice>
      )}
    </Section>
  );
}

function ControlsSection() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const firing = useQuery({
    queryKey: oneButtonFiringKey,
    queryFn: () => unwrap(commands.oneButtonFiring()),
  });
  const update = useMutation({
    mutationFn: (on: boolean) => unwrap(commands.setOneButtonFiring(on)),
    onSettled: () => queryClient.invalidateQueries({ queryKey: oneButtonFiringKey }),
  });

  return (
    <Section title={t("settings.controls")}>
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
          <p className="ml-6 text-xs text-muted">{t("settings.oneButtonFiringHint")}</p>
        </div>
      )}
      {(firing.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, firing.error ?? update.error)}</Notice>
      )}
    </Section>
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
    <Section title={t("settings.backups")}>
      {s && (
        <>
          <label className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              checked={s.auto}
              onChange={(e) => update.mutate({ ...s, auto: e.target.checked })}
            />
            {t("settings.autoBackup")}
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
          <p className="text-xs text-muted">{t("settings.backupsHint")}</p>
        </>
      )}
      {(settings.error ?? update.error) && (
        <Notice tone="error">{errorMessage(t, settings.error ?? update.error)}</Notice>
      )}
    </Section>
  );
}

function GlobalConfigSection() {
  const { t } = useTranslation();
  const view = useQuery({
    queryKey: configKey(null),
    queryFn: () => unwrap(commands.configView(null)),
  });
  return (
    <Section title={t("config.globalTitle")}>
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
    </Section>
  );
}

const selectClass = "rounded-lg border border-border bg-surface-raised px-3 py-2 text-sm text-ink";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card className="p-5">
      <h2 className="mb-4 font-semibold">{title}</h2>
      <div className="grid gap-4">{children}</div>
    </Card>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-wrap items-center justify-between gap-3 text-sm">
      <span>{label}</span>
      {children}
    </label>
  );
}
