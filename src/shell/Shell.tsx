import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { Compass, LibraryBig, Settings, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useIndexEvents } from "../discover/queries";
import { JobsPanel } from "../jobs/JobsPanel";
import { useJobEvents } from "../jobs/queries";

/**
 * App frame (PLAN.md §8.2): full sidebar from 1200px, icon rail below. The
 * setup wizard gets the whole window.
 */
export function Shell() {
  const { t } = useTranslation();
  const inSetup = useRouterState({ select: (s) => s.location.pathname === "/setup" });
  useJobEvents();
  useIndexEvents();

  if (inSetup) {
    return (
      <main className="bb-starfield min-h-screen overflow-y-auto">
        <Outlet />
      </main>
    );
  }
  return (
    <div className="flex h-screen">
      <nav
        aria-label={t("nav.label")}
        className="flex w-16 shrink-0 flex-col gap-1 border-r border-border bg-surface p-2 xl:w-60 xl:p-3"
      >
        <div className="mb-4 flex h-12 items-center justify-center font-pixel text-accent xl:justify-start xl:px-2">
          <span className="text-sm xl:hidden" aria-hidden>
            BB
          </span>
          <span className="hidden text-sm xl:inline">BloogBase</span>
        </div>
        <NavItem to="/" icon={LibraryBig} label={t("nav.library")} />
        <NavItem to="/discover" icon={Compass} label={t("nav.discover")} />
        <div className="mt-auto grid gap-1">
          <JobsPanel />
          <NavItem to="/settings" icon={Settings} label={t("nav.settings")} />
        </div>
      </nav>
      <main className="min-w-0 flex-1 overflow-y-auto">
        <div className="mx-auto max-w-7xl p-6 xl:p-8">
          <Outlet />
        </div>
      </main>
    </div>
  );
}

function NavItem({
  to,
  icon: Icon,
  label,
}: {
  to: "/" | "/discover" | "/settings";
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
