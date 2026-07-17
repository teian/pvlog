import { useState, type PropsWithChildren } from "react";
import { useTranslation } from "react-i18next";
import { AppSidebar } from "./AppSidebar";
import { AdministrationSidebar } from "./AdministrationSidebar";
import { MobileNavigationHeader } from "./MobileNavigationHeader";

/** Responsive application shell properties. */
export interface AppShellProps extends PropsWithChildren {
  /** Systems available to the user. */ systemIds?: string[] | undefined;
  /** Navigation and layout context rendered by the shell. */
  variant?: "application" | "administration";
}

/** Renders the sidebar-owned application frame and main content. @param props - Shell content and navigation context. @returns The responsive shell. */
export function AppShell({
  children,
  systemIds = [],
  variant = "application",
}: AppShellProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  return (
    <div className="min-h-screen bg-background text-foreground">
      <a
        className="sr-only focus:fixed focus:left-4 focus:top-4 focus:z-40 focus:rounded-md focus:bg-primary focus:px-4 focus:py-2 focus:text-primary-foreground focus:not-sr-only"
        href="#main-content"
      >
        {t("shell.skip")}
      </a>
      {open ? (
        <button
          aria-label={t("shell.closeNavigation")}
          className="fixed inset-0 z-20 bg-foreground/25 md:hidden"
          onClick={() => {
            setOpen(false);
          }}
          type="button"
        />
      ) : null}
      {variant === "administration" ? (
        <AdministrationSidebar
          onClose={() => {
            setOpen(false);
          }}
          open={open}
          systemIds={systemIds}
        />
      ) : (
        <AppSidebar
          onClose={() => {
            setOpen(false);
          }}
          open={open}
          systemIds={systemIds}
        />
      )}
      <div className="min-h-screen md:ml-[15.75rem]">
        <MobileNavigationHeader
          onOpen={() => {
            setOpen(true);
          }}
        />
        <main
          className="mx-auto flex max-w-screen-xl flex-col gap-6 px-4 py-6 sm:px-6 md:px-8 md:py-8"
          id="main-content"
        >
          {children}
        </main>
      </div>
    </div>
  );
}
