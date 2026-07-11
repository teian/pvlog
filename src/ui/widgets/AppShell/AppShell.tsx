import { Button } from "@/shared/components";
import { cn } from "@/shared/lib/utils";
import { MenuIcon, PanelLeftCloseIcon, SunMediumIcon } from "lucide-react";
import { useState, type PropsWithChildren } from "react";
import { useTranslation } from "react-i18next";
import { Link, NavLink } from "react-router";
import { SessionControls } from "./SessionControls";

/** Responsive application shell properties. */
export interface AppShellProps extends PropsWithChildren {
  /** Account shown in navigation. */ accountId?: string | null | undefined;
  /** Systems available to the user. */ systemIds?: string[] | undefined;
}

/** Renders account/system navigation, skip link, header, and main content. @param props - Shell content and navigation context. @returns The responsive shell. */
export function AppShell({
  accountId,
  children,
  systemIds = [],
}: AppShellProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  return (
    <div className="min-h-screen bg-background text-foreground">
      <a
        className="sr-only focus:not-sr-only focus:fixed focus:left-4 focus:top-4 focus:rounded-md focus:bg-primary focus:px-4 focus:py-2 focus:text-primary-foreground"
        href="#main-content"
      >
        {t("shell.skip")}
      </a>
      <aside
        className={cn(
          "fixed inset-y-0 left-0 w-60 border-r border-sidebar-border bg-sidebar p-4 md:block",
          open ? "block" : "hidden",
        )}
      >
        <div className="flex items-center justify-between">
          <Link className="font-bold tracking-tight" to="/">
            {t("home.title")}
          </Link>
          <Button
            aria-label={t("shell.closeNavigation")}
            className="md:hidden"
            onClick={() => {
              setOpen(false);
            }}
            size="icon"
            variant="ghost"
          >
            <PanelLeftCloseIcon />
          </Button>
        </div>
        <nav
          aria-label={t("shell.primaryNavigation")}
          className="mt-6 flex flex-col gap-2"
        >
          <NavLink
            className="rounded-md px-3 py-2 text-sm transition-colors hover:bg-sidebar-accent"
            to="/"
          >
            {t("nav.dashboard")}
          </NavLink>
          <NavLink
            className="rounded-md px-3 py-2 text-sm transition-colors hover:bg-sidebar-accent"
            to="/alerts"
          >
            {t("nav.alerts")}
          </NavLink>
          <NavLink
            className="rounded-md px-3 py-2 text-sm transition-colors hover:bg-sidebar-accent"
            to="/administration"
          >
            {t("nav.administration")}
          </NavLink>
          {accountId ? (
            <p className="mt-4 truncate text-xs font-semibold uppercase tracking-widest text-muted-foreground">
              {t("nav.account", { id: accountId.slice(0, 8) })}
            </p>
          ) : null}
          {systemIds.map((id) => (
            <NavLink
              className="truncate rounded-md px-3 py-2 font-mono text-xs transition-colors hover:bg-sidebar-accent"
              key={id}
              to={`/systems/${id}`}
            >
              {t("nav.system", { id: id.slice(0, 8) })}
            </NavLink>
          ))}
        </nav>
      </aside>
      <div className="md:ml-60">
        <header className="flex h-14 items-center justify-between border-b border-border px-4">
          <Button
            aria-label={t("shell.openNavigation")}
            className="md:hidden"
            onClick={() => {
              setOpen(true);
            }}
            size="icon"
            variant="ghost"
          >
            <MenuIcon />
          </Button>
          <span className="text-sm font-medium">{t("shell.title")}</span>
          <Button aria-label={t("shell.theme")} size="icon" variant="ghost">
            <SunMediumIcon />
          </Button>
          <SessionControls />
        </header>
        <main
          className="mx-auto flex max-w-screen-xl flex-col gap-6 px-6 py-6"
          id="main-content"
        >
          {children}
        </main>
      </div>
    </div>
  );
}
