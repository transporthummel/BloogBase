import { useTranslation } from "react-i18next";
import { REPO_URL } from "../about/About";
import { commands } from "../bindings";
import { unwrap } from "../ipc";
import { ReportDialog } from "../ui/ReportDialog";

/** The new issue for a mod that doesn't start, its title filled in. */
function modIssueUrl(name: string) {
  const title = `Mod doesn't start: ${name}`;
  return `${REPO_URL}/issues/new?title=${encodeURIComponent(title)}`;
}

/** What a report about a mod that doesn't start needs, to copy into a new issue. */
export function ModReportDialog({
  profileId,
  name,
  onClose,
}: {
  profileId: string;
  name: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  return (
    <ReportDialog
      title={t("modReport.title", { name })}
      hint={t("modReport.hint")}
      queryKey={["modReport", profileId]}
      load={() => unwrap(commands.modReport(profileId))}
      issueUrl={modIssueUrl(name)}
      onClose={onClose}
    />
  );
}
