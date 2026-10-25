import { useQuery } from "@tanstack/react-query";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ExternalLink, FileText } from "lucide-react";
import { useState, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import logo from "../assets/logo.jpg";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { useRuntimeStatus } from "../runtime/queries";
import { Bloog } from "../ui/Bloog";
import { Button } from "../ui/Button";
import { Card } from "../ui/Card";
import { Dialog } from "../ui/Dialog";
import { Loading } from "../ui/Loading";
import { Notice } from "../ui/Notice";

export const REPO_URL = "https://github.com/transporthummel/BloogBase";
const DOSBOX_URL = "https://www.dosbox-staging.org/";
const DOSBOX_SOURCE = "https://github.com/dosbox-staging/dosbox-staging";
const GPL_URL = "https://www.gnu.org/licenses/old-licenses/gpl-2.0.html";
const CWSDPMI_SOURCE = "https://www.delorie.com/pub/djgpp/current/v2misc/csdpmi7s.zip";
const CKPATCH_SOURCE =
  "https://files.shikadi.net/keenwiki/tools/t.ckpatch_v.0.10_src.andrew_durdin.2007-11-23.zip";
const AUTHOR_URL = "https://pckf.com/memberlist.php?mode=viewprofile&u=16709";

/** Version, credits, licenses and the trademark notice (PLAN.md §13 M8, §14). */
export function About() {
  const { t } = useTranslation();
  const info = useQuery({ queryKey: ["appInfo"], queryFn: () => unwrap(commands.appInfo()) });
  const runtime = useRuntimeStatus();
  const [showLicenses, setShowLicenses] = useState(false);

  // The source of the exact DOSBox version BloogBase downloads (GPL §3).
  const dosboxVersion = runtime.data?.managed?.version ?? runtime.data?.pinnedVersion;

  return (
    <div className="grid max-w-3xl gap-6">
      <Card>
        <img
          src={logo}
          alt="BloogBase"
          className="aspect-[16/9] w-full object-cover [image-rendering:pixelated]"
        />
        <div className="flex items-start gap-4 p-5">
          <div className="grid flex-1 gap-2">
            <h1 className="sr-only">{t("about.title")}</h1>
            <p>{t("about.tagline")}</p>
            <p className="text-sm">
              <Trans
                i18nKey="about.author"
                values={{ name: "transporthummel" }}
                components={{
                  profile: (
                    <button
                      type="button"
                      className="font-semibold text-accent underline-offset-2 hover:underline"
                      title={t("about.authorProfile")}
                      onClick={() => openUrl(AUTHOR_URL)}
                    />
                  ),
                }}
              />
            </p>
            <p className="text-sm text-muted">
              {t("settings.version")}{" "}
              <span className="font-pixel text-xs text-ink">{info.data?.version}</span>
            </p>
            <div className="flex flex-wrap gap-2 pt-1">
              <LinkButton url={REPO_URL}>{t("about.sourceCode")}</LinkButton>
              <LinkButton url={`${REPO_URL}/issues`}>{t("about.reportProblem")}</LinkButton>
            </div>
          </div>
          <Bloog className="bb-hop size-10 shrink-0" />
        </div>
      </Card>

      <Section title={t("about.fanProject")}>
        <p>{t("about.trademark")}</p>
        <p>{t("about.noGameData")}</p>
      </Section>

      <Section title={t("about.thanks")}>
        <p>{t("about.thanksText")}</p>
        <div className="flex flex-wrap gap-2">
          <LinkButton url="https://keenwiki.shikadi.net/">KeenWiki</LinkButton>
          <LinkButton url="https://pckf.com/">PCKF</LinkButton>
          <LinkButton url="https://moddingwiki.shikadi.net/">ModdingWiki</LinkButton>
        </div>
      </Section>

      <Section title={t("about.licenses")}>
        <p>{t("about.license")}</p>
        <p>
          <Trans
            i18nKey="about.dosbox"
            values={{ version: dosboxVersion ?? "" }}
            components={{ strong: <strong /> }}
          />
        </p>
        <div className="flex flex-wrap gap-2">
          <LinkButton url={DOSBOX_URL}>DOSBox Staging</LinkButton>
          <LinkButton
            url={dosboxVersion ? `${DOSBOX_SOURCE}/tree/v${dosboxVersion}` : DOSBOX_SOURCE}
          >
            {t("about.dosboxSource", { version: dosboxVersion ?? "" })}
          </LinkButton>
          <LinkButton url={GPL_URL}>GPL 2.0</LinkButton>
        </div>
        <p>{t("about.cwsdpmi")}</p>
        <div className="flex flex-wrap gap-2">
          <LinkButton url={CWSDPMI_SOURCE}>{t("about.cwsdpmiSource")}</LinkButton>
        </div>
        <p>{t("about.ckpatch")}</p>
        <div className="flex flex-wrap gap-2">
          <LinkButton url={CKPATCH_SOURCE}>{t("about.ckpatchSource")}</LinkButton>
        </div>
        <p>{t("about.fonts")}</p>
        <div>
          <Button variant="secondary" onClick={() => setShowLicenses(true)}>
            <FileText className="size-4" aria-hidden />
            {t("about.thirdParty")}
          </Button>
        </div>
      </Section>

      {showLicenses && <LicensesDialog onClose={() => setShowLicenses(false)} />}
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card className="grid gap-3 p-5 text-sm leading-relaxed">
      <h2 className="font-pixel text-xs leading-relaxed">{title}</h2>
      {children}
    </Card>
  );
}

function LinkButton({ url, children }: { url: string; children: ReactNode }) {
  return (
    <Button variant="secondary" className="px-3 py-1.5" onClick={() => openUrl(url)}>
      <ExternalLink className="size-4" aria-hidden />
      {children}
    </Button>
  );
}

/** The generated list of open source components and their license texts. */
function LicensesDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const text = useQuery({
    queryKey: ["thirdPartyLicenses"],
    queryFn: async () => {
      const res = await fetch("/third-party-licenses.txt");
      const body = res.ok ? await res.text() : "";
      // Dev servers answer unknown paths with the app's HTML.
      return body.startsWith("BloogBase") ? body : null;
    },
    staleTime: Infinity,
  });
  return (
    <Dialog wide title={t("about.thirdParty")} onClose={onClose}>
      {text.isPending && <Loading />}
      {text.data === null && <Notice>{t("about.licensesMissing")}</Notice>}
      {text.data && (
        <pre className="max-h-[65vh] overflow-auto rounded-lg bg-surface-raised p-3 text-xs whitespace-pre-wrap">
          {text.data}
        </pre>
      )}
      <div className="flex justify-end">
        <Button variant="secondary" onClick={onClose}>
          {t("common.close")}
        </Button>
      </div>
    </Dialog>
  );
}
