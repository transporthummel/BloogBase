import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";
import { commands } from "../bindings";
import { ConfigEditor, ConfigText } from "../config/LazyConfigEditor";
import { configKey } from "../config/queries";
import { errorMessage } from "../errors";
import { unwrap } from "../ipc";
import { Disclosure } from "../ui/Disclosure";
import { Notice } from "../ui/Notice";
import { CustomPatchesSection } from "./CustomPatches";

/**
 * A profile's settings: its DOSBox settings with its own layer, then folded
 * away the result and the defaults (§6.1) and custom patches, which few
 * players need.
 */
export function ConfigPanel({ profileId }: { profileId: string }) {
  const { t } = useTranslation();
  const view = useQuery({
    queryKey: configKey(profileId),
    queryFn: () => unwrap(commands.configView(profileId)),
  });
  const p = view.data?.profile;

  return (
    <div className="grid gap-6">
      {view.error && <Notice tone="error">{errorMessage(t, view.error)}</Notice>}
      <section className="grid gap-2" aria-labelledby="config-own">
        <h2 id="config-own" className="font-semibold">
          {t("config.profileTitle")}
        </h2>
        <p className="text-sm text-muted">
          {t("config.profileHint")}{" "}
          <Link to="/settings" className="underline">
            {t("config.globalLink")}
          </Link>
        </p>
        {p && (
          <ConfigEditor
            profileId={profileId}
            text={p.text}
            warnings={p.warnings}
            label={t("config.profileTitle")}
          />
        )}
      </section>

      {p && (
        <Disclosure title={t("config.effectiveTitle")}>
          <p className="text-sm text-muted">{t("config.effectiveHint")}</p>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead className="text-xs text-muted">
                <tr>
                  <th className="py-1 pr-4 font-normal">{t("config.setting")}</th>
                  <th className="py-1 pr-4 font-normal">{t("config.value")}</th>
                  <th className="py-1 font-normal">{t("config.from")}</th>
                </tr>
              </thead>
              <tbody>
                {p.effective.map((s) => (
                  <tr key={`${s.section}.${s.key}`} className="border-t border-border">
                    <td className="py-1 pr-4 font-mono text-xs">
                      [{s.section}] {s.key}
                    </td>
                    <td className="py-1 pr-4 font-mono text-xs [overflow-wrap:anywhere] select-text">
                      {s.value}
                    </td>
                    <td className="py-1 text-xs whitespace-nowrap text-muted">
                      {t(`config.layer.${s.layer}`)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </Disclosure>
      )}
      {p && (
        <Disclosure title={t("config.defaultsTitle")}>
          <ConfigText text={p.defaults} label={t("config.defaultsTitle")} />
        </Disclosure>
      )}
      <CustomPatchesSection profileId={profileId} />
    </div>
  );
}
