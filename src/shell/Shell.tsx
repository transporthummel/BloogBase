import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { Compass, Info, LibraryBig, Settings, type LucideIcon } from "lucide-react";
import icon from "../assets/icon.png";
import { useTranslation } from "react-i18next";
import { useIndexEvents } from "../discover/queries";
import { JobsPanel } from "../jobs/JobsPanel";
import { DropImport } from "../library/DropImport";
import { useJobEvents } from "../jobs/queries";
import { hideSplash } from "../splash";
import { useAppUpdate } from "../updates/queries";
import { UpdateCard } from "../updates/UpdateCard";
import { PageFooterContext } from "./pageFooterContext";

/**
 * App frame (PLAN.md §8.2): full sidebar from 1200px, icon rail below. The
 * setup wizard gets the whole window.
 */
export function Shell() {
  const { t } = useTranslation();
  const inSetup = useRouterState({ select: (s) => s.location.pathname === "/setup" });
  useJobEvents();
  useIndexEvents();
  // Release builds look for a new version once per start.
  const update = useAppUpdate({ enabled: import.meta.env.PROD });
  const [updateLater, setUpdateLater] = useState(false);
  const [footer, setFooter] = useState<HTMLElement | null>(null);
  useEffect(hideSplash, []);

  if (inSetup) {
    return (
      <main className="bb-starfield relative min-h-screen overflow-y-auto">
        <Outlet />
      </main>
    );
  }
  return (
    <div className="flex h-screen">
      <DropImport />
      <nav
        aria-label={t("nav.label")}
        className="flex w-16 shrink-0 flex-col gap-1 border-r border-border bg-surface p-2 xl:w-60 xl:p-3"
      >
        <Link
          to="/about"
          title={t("nav.about")}
          className="mb-4 flex h-12 items-center justify-center gap-3 rounded-lg xl:justify-start xl:px-1"
        >
          <img src={icon} alt="" className="size-10 shrink-0" />
          <span className="sr-only font-pixel text-xs text-accent xl:not-sr-only">BloogBase</span>
        </Link>
        <NavItem to="/" icon={LibraryBig} label={t("nav.library")} />
        <NavItem to="/discover" icon={Compass} label={t("nav.discover")} />
        <div className="mt-auto grid gap-1">
          <JobsPanel />
          <NavItem to="/settings" icon={Settings} label={t("nav.settings")} />
          <NavItem to="/about" icon={Info} label={t("nav.about")} />
        </div>
      </nav>
      <main className="relative flex min-w-0 flex-1 flex-col overflow-y-auto">
        <div className="mx-auto w-full max-w-7xl flex-1 p-6 xl:p-8">
          {update.data && !updateLater && (
            <div className="mb-6">
              <UpdateCard update={update.data} onLater={() => setUpdateLater(true)} />
            </div>
          )}
          <PageFooterContext.Provider value={footer}>
            <Outlet />
          </PageFooterContext.Provider>
        </div>
        {/* Pages put full-width bars here (PageFooter). */}
        <div ref={setFooter} className="sticky bottom-0 empty:hidden" />
      </main>
    </div>
  );
}

function NavItem({
  to,
  icon: Icon,
  label,
}: {
  to: "/" | "/discover" | "/settings" | "/about";
  icon: LucideIcon;
  label: string;
}) {
  return (
    <Link
      to={to}
      title={label}
      className="flex h-12 items-center justify-center gap-3 rounded-lg text-muted transition hover:bg-surface-raised hover:text-ink xl:justify-start xl:px-3"
      activeProps={{ className: "bg-surface-raised text-ink", "aria-current": "page" }}
      activeOptions={{ exact: true }}
    >
      <Icon className="size-5 shrink-0" aria-hidden />
      <span className="sr-only xl:not-sr-only xl:text-sm xl:font-medium">{label}</span>
    </Link>
  );
}
