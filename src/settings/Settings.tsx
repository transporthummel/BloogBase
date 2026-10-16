import { useQuery } from "@tanstack/react-query";
import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { getTheme, setLanguage, setTheme, type Language, type Theme } from "../preferences";
import { Card } from "../ui/Card";
import { PageHeader } from "../ui/PageHeader";

export function Settings() {
  const { t, i18n } = useTranslation();
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
