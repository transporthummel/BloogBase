import { Link, Outlet } from "@tanstack/react-router";
import { LibraryBig, Settings, type LucideIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

/**
 * App frame (PLAN.md §8.2): full sidebar from 1200px, icon rail below.
 */
export function Shell() {
  const { t } = useTranslation();
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
        <div className="mt-auto">
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
  to: "/" | "/settings";
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
