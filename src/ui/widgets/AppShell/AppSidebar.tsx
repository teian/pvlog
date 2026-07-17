import { Button } from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { ListTreeIcon, PanelLeftCloseIcon, SettingsIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router";
import { AppBrand } from "./AppBrand";
import { ApplicationViewNavigation } from "./ApplicationViewNavigation";
import { SessionControls } from "./SessionControls";

/** Application sidebar properties. */
export interface AppSidebarProps {
  /** Closes the mobile sidebar. */
  onClose: () => void;
  /** Whether the mobile sidebar is open. */
  open: boolean;
  /** Systems available to the signed-in user. */
  systemIds: string[];
}

const navLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex items-center gap-3 rounded-md border-l-2 border-transparent px-3 py-2.5 text-sm text-sidebar-foreground/80 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
    isActive &&
      "border-brand bg-sidebar-accent font-semibold text-sidebar-accent-foreground",
  );

const scopeLinkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex items-center gap-3 rounded-md border-l-2 border-transparent px-3 py-2.5 text-sm text-sidebar-foreground/80 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
    isActive &&
      "border-brand bg-sidebar-selected font-semibold text-sidebar-accent-foreground",
  );

/** Renders the complete desktop navigation and mobile drawer. @param props - Sidebar state and available scope. @returns The application sidebar. */
export function AppSidebar({ onClose, open, systemIds }: AppSidebarProps) {
  const { t } = useTranslation();
  const firstSystemId = systemIds.at(0);

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
          aria-label={t("shell.primaryNavigation")}
          className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-2 py-4"
        >
          <p className="px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-widest text-sidebar-foreground/70">
            {t("nav.view")}
          </p>
          <ApplicationViewNavigation
            firstSystemId={firstSystemId}
            onClose={onClose}
          />
          <p className="mt-5 px-2 pb-1.5 text-[10px] font-semibold uppercase tracking-widest text-sidebar-foreground/70">
            {t("nav.systems")}
          </p>
          <NavLink className={scopeLinkClass} end onClick={onClose} to="/">
            <ListTreeIcon aria-hidden="true" className="size-4" />
            {t("nav.allSystems")}
          </NavLink>
          {systemIds.map((id) => (
            <NavLink
              className={({ isActive }) =>
                cn(scopeLinkClass({ isActive }), "font-mono text-xs")
              }
              key={id}
              onClick={onClose}
              to={`/systems/${id}`}
            >
              {t("nav.system", { id: id.slice(0, 8) })}
            </NavLink>
          ))}
        </nav>
        <div className="shrink-0 border-t border-sidebar-border px-2 py-3">
          <NavLink
            className={navLinkClass}
            onClick={onClose}
            to="/administration"
          >
            <SettingsIcon aria-hidden="true" className="size-4" />
            {t("nav.administration")}
          </NavLink>
          <SessionControls />
          <p className="px-2 pt-3 text-[10px] text-sidebar-foreground/70">
            {t("shell.footer")}
          </p>
        </div>
      </div>
    </aside>
  );
}
