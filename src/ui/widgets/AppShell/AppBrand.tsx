import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { cn } from "@/shared/lib/utils";

/** Visual sizes supported by the shared PVLog lockup. */
export type AppBrandSize = "compact" | "hero";

/** Shared PVLog lockup properties. */
export interface AppBrandProps {
  /** Enlarges the mark for authentication and marketing surfaces. */
  size?: AppBrandSize;
}

/** Renders the compact PVLog mark and open-monitoring wordmark. @returns The linked application brand. */
export function AppBrand({ size = "compact" }: AppBrandProps) {
  const { t } = useTranslation();
  const isHero = size === "hero";

  return (
    <Link
      className={cn(
        "flex items-center text-sidebar-foreground",
        isHero ? "gap-4" : "gap-3",
      )}
      to="/"
    >
      <img
        aria-hidden="true"
        alt=""
        className={cn("shrink-0", isHero ? "size-10" : "size-8")}
        src="/pvlog-logo.svg"
      />
      <span className="flex min-w-0 flex-col">
        <span
          className={cn(
            "font-extrabold tracking-tight",
            isHero ? "text-xl" : "text-sm",
          )}
        >
          {t("home.title")}
        </span>
        <span
          className={cn(
            "truncate font-semibold uppercase tracking-[0.14em] text-sidebar-foreground/70",
            isHero ? "text-[10px]" : "text-[9px]",
          )}
        >
          {t("shell.brandTagline")}
        </span>
      </span>
    </Link>
  );
}
