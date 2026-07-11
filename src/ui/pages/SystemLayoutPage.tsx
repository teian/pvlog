import { useSession } from "@/features/auth";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";
import { NavLink, Outlet } from "react-router";

const TAB_CLASS =
  "border-b-2 -mb-px px-4 py-2.5 text-sm font-medium transition-colors";

/** Renders the system detail shell (charts, data quality, and future tabs) with shared navigation. @returns The system layout page. */
export function SystemLayoutPage() {
  const { t } = useTranslation();
  const session = useSession();

  return (
    <AppShell
      accountId={session.data?.accountId}
      systemIds={session.data?.systemIds}
    >
      <section className="flex flex-col gap-6">
        <header className="border-b border-border">
          <nav aria-label={t("system.tabsLabel")} className="-mb-px flex gap-2">
            <NavLink
              className={({ isActive }) =>
                `${TAB_CLASS} ${
                  isActive
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:border-border hover:text-foreground"
                }`
              }
              end
              to="."
            >
              {t("system.tabs.charts")}
            </NavLink>
            <NavLink
              className={({ isActive }) =>
                `${TAB_CLASS} ${
                  isActive
                    ? "border-primary text-foreground"
                    : "border-transparent text-muted-foreground hover:border-border hover:text-foreground"
                }`
              }
              to="data-quality"
            >
              {t("system.tabs.dataQuality")}
            </NavLink>
          </nav>
        </header>
        <Outlet />
      </section>
    </AppShell>
  );
}
