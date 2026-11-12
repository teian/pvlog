import { mayOpen, useSession } from "@/features/auth";
import { useSystemOverviews } from "@/features/reporting";
import { Button } from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import {
  ListTreeIcon,
  PanelLeftCloseIcon,
  PlusIcon,
  SettingsIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link, NavLink } from "react-router";
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
  const permissions = useSession().data?.permissions;
  // A system page needs its own permission; without it the names would be
  // links to the forbidden page.
  const listedSystemIds = mayOpen(permissions, "system") ? systemIds : [];
  // Names come from the cached system overviews; the identifier stays as the
  // label until one resolves, so navigation never renders an empty entry.
  const overviews = useSystemOverviews(listedSystemIds);

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
          <ApplicationViewNavigation onClose={onClose} />
          <div className="mt-5 flex items-center justify-between px-2 pb-1.5">
            <p className="text-[10px] font-semibold uppercase tracking-widest text-sidebar-foreground/70">
              {t("nav.systems")}
            </p>
            {mayOpen(permissions, "setup") ? (
              <Link
                className="flex items-center gap-1 rounded-md px-1.5 py-0.5 text-xs font-semibold text-sidebar-foreground/80 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
                onClick={onClose}
                to="/setup"
              >
                <PlusIcon aria-hidden="true" className="size-3.5" />
                {t("nav.addSystem")}
              </Link>
            ) : null}
          </div>
          <NavLink className={scopeLinkClass} end onClick={onClose} to="/">
            <ListTreeIcon aria-hidden="true" className="size-4" />
            {t("nav.allSystems")}
          </NavLink>
          {listedSystemIds.map((id, index) => {
            const overview = overviews[index]?.data;
            const name = overview?.name;
            // Only an active system that is not delivering is marked; a
            // working one needs no decoration.
            const missing =
              overview?.lifecycle === "active" &&
              overview.dataState !== "current"
                ? overview.dataState
                : null;
            return (
              <NavLink
                className={({ isActive }) =>
                  cn(scopeLinkClass({ isActive }), !name && "font-mono text-xs")
                }
                key={id}
                onClick={onClose}
                title={name}
                to={`/systems/${id}`}
              >
                <span className="truncate">
                  {name ?? t("nav.system", { id: id.slice(0, 8) })}
                </span>
                {missing ? (
                  <>
                    <span
                      aria-hidden="true"
                      className="ml-auto size-2 shrink-0 rounded-full bg-warning"
                    />
                    <span className="sr-only">
                      {t(`system.dataState.${missing}`)}
                    </span>
                  </>
                ) : null}
              </NavLink>
            );
          })}
        </nav>
        <div className="shrink-0 border-t border-sidebar-border px-2 py-3">
          {mayOpen(permissions, "administration") ? (
            <NavLink
              className={navLinkClass}
              onClick={onClose}
              to="/administration"
            >
              <SettingsIcon aria-hidden="true" className="size-4" />
              {t("nav.administration")}
            </NavLink>
          ) : null}
          <SessionControls />
          <p className="px-2 pt-3 text-[10px] text-sidebar-foreground/70">
            {t("shell.footer")}
          </p>
        </div>
      </div>
    </aside>
  );
}
