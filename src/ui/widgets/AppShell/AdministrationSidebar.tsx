import { Button } from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { ChevronLeftIcon, PanelLeftCloseIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { AppBrand } from "./AppBrand";
import type { AppSidebarProps } from "./AppSidebar";
import { SessionControls } from "./SessionControls";

const sections = [
  "users",
  "data-sources",
  "alert-rules",
  "notifications",
  "retention-backup",
  "system-logs",
] as const;

/** Renders the focused navigation used throughout administration. */
export function AdministrationSidebar({ onClose, open }: AppSidebarProps) {
  const { t } = useTranslation();
  const [searchParams] = useSearchParams();
  const activeSection = searchParams.get("section") ?? "users";

  return (
    <aside
      className={cn(
        "fixed inset-y-0 left-0 z-30 w-[15.75rem] border-r border-sidebar-border bg-sidebar text-sidebar-foreground shadow-xl md:block md:shadow-none",
        open ? "block" : "hidden",
      )}
    >
      <div className="flex h-full flex-col">
        <div className="flex h-[4.25rem] shrink-0 items-center justify-between border-b border-sidebar-border px-4">
          <AppBrand />
          <Button
            aria-label={t("shell.closeNavigation")}
            className="text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground md:hidden"
            onClick={onClose}
            size="icon"
            variant="ghost"
          >
            <PanelLeftCloseIcon />
          </Button>
        </div>
        <nav
          aria-label={t("administration.navigation.label")}
          className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2 py-4"
        >
          <p className="px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-widest text-sidebar-foreground/70">
            {t("administration.navigation.label")}
          </p>
          {sections.map((section) => {
            const active = activeSection === section;
            return (
              <Link
                aria-current={active ? "page" : undefined}
                className={cn(
                  "rounded-md border-l-2 border-transparent px-2 py-2.5 text-sm text-sidebar-foreground/80 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
                  active &&
                    "border-brand bg-sidebar-accent font-semibold text-sidebar-accent-foreground",
                )}
                key={section}
                onClick={onClose}
                to={`/administration?section=${section}`}
              >
                {t(`administration.navigation.${section}`)}
              </Link>
            );
          })}
        </nav>
        <div className="shrink-0 border-t border-sidebar-border px-2 py-3">
          <Link
            className="flex items-center gap-3 rounded-md px-3 py-2.5 text-sm font-semibold text-sidebar-foreground transition-colors hover:bg-sidebar-accent"
            onClick={onClose}
            to="/"
          >
            <ChevronLeftIcon aria-hidden="true" className="size-4 text-brand" />
            {t("administration.navigation.leave")}
          </Link>
          <SessionControls />
          <p className="px-2 pt-3 text-[10px] text-sidebar-foreground/70">
            {t("shell.footer")}
          </p>
        </div>
      </div>
    </aside>
  );
}
