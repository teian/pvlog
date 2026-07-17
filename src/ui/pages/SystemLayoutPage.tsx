import { useSession } from "@/features/auth";
import { cn } from "@/shared/lib/utils";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";
import { NavLink, Outlet } from "react-router";

const tabClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "rounded-md px-4 py-2 text-sm font-medium transition-colors",
    isActive
      ? "bg-card text-primary shadow-xs"
      : "text-muted-foreground hover:bg-card/60 hover:text-foreground",
  );

/** Renders the system detail shell (charts, data quality, and future tabs) with shared navigation. @returns The system layout page. */
export function SystemLayoutPage() {
  const { t } = useTranslation();
  const session = useSession();

  return (
    <AppShell systemIds={session.data?.systemIds}>
      <section className="flex flex-col gap-6">
        <header>
          <nav
            aria-label={t("system.tabsLabel")}
            className="flex w-fit max-w-full flex-wrap gap-1 rounded-lg bg-muted p-1"
          >
            <NavLink className={tabClass} end to=".">
              {t("system.tabs.charts")}
            </NavLink>
            <NavLink className={tabClass} to="forecast">
              {t("forecasting.page.tab")}
            </NavLink>
            <NavLink className={tabClass} to="data-quality">
              {t("system.tabs.dataQuality")}
            </NavLink>
          </nav>
        </header>
        <Outlet />
      </section>
    </AppShell>
  );
}
