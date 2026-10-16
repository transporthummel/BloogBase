import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { useNavigate } from "@tanstack/react-router";
import { FileCog, Wand } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import {
  getTheme,
  setLanguage,
  setSetupStep,
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
        </Section>
        <RuntimeSection />
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
        <Section title={t("settings.about")}>
          <dl className="grid grid-cols-[auto_1fr] gap-x-6 gap-y-2 text-sm">
            <dt className="text-muted">{t("settings.version")}</dt>
            <dd className="font-pixel text-xs leading-5">{info.data?.version}</dd>
            <dt className="text-muted">{t("settings.dataDir")}</dt>
            <dd className="break-all">{info.data?.dataDir}</dd>
          </dl>
        </Section>
      </div>
    </>
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
