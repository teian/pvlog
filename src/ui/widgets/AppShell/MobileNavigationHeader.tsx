import { Button } from "@/shared/components";
import { MenuIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { AppBrand } from "./AppBrand";

/** Mobile navigation header properties. */
export interface MobileNavigationHeaderProps {
  /** Opens the off-canvas application sidebar. */
  onOpen: () => void;
}

/** Provides branding and a navigation trigger on narrow screens. @param props - Mobile navigation actions. @returns The mobile-only header. */
export function MobileNavigationHeader({
  onOpen,
}: MobileNavigationHeaderProps) {
  const { t } = useTranslation();

  return (
    <header className="flex h-[4.25rem] items-center justify-between border-b border-sidebar-border bg-sidebar px-4 text-sidebar-foreground md:hidden">
      <AppBrand />
      <Button
        aria-label={t("shell.openNavigation")}
        className="text-sidebar-foreground hover:bg-sidebar-accent hover:text-sidebar-accent-foreground"
        onClick={onOpen}
        size="icon"
        variant="ghost"
      >
        <MenuIcon />
      </Button>
    </header>
  );
}
