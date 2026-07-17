import { cn } from "@/shared/lib/utils";
import {
  BarChart3Icon,
  BriefcaseIcon,
  CloudSunIcon,
  LayoutDashboardIcon,
  ListIcon,
  SunIcon,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router";

interface ViewNavigationItem {
  end?: boolean;
  icon: LucideIcon;
  labelKey: string;
  to: string;
}

const linkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "flex items-center gap-3 rounded-md border-l-2 border-transparent px-3 py-2.5 text-sm text-sidebar-foreground/80 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground",
    isActive &&
      "border-brand bg-sidebar-accent font-semibold text-sidebar-accent-foreground",
  );

/** Renders the six design-defined application views. @param props - Current system scope and drawer close callback. @returns Localized view navigation. */
export function ApplicationViewNavigation({
  onClose,
}: {
  firstSystemId?: string | undefined;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const items: ViewNavigationItem[] = [
    { end: true, icon: LayoutDashboardIcon, labelKey: "dashboard", to: "/" },
    {
      end: true,
      icon: ListIcon,
      labelKey: "systems",
      to: "/systems",
    },
    {
      icon: BarChart3Icon,
      labelKey: "statistics",
      to: "/statistics",
    },
    { icon: SunIcon, labelKey: "seasonal", to: "/seasonal" },
    { icon: CloudSunIcon, labelKey: "weather", to: "/weather" },
    { icon: BriefcaseIcon, labelKey: "manage", to: "/onboarding" },
  ];

  return items.map(({ end, icon: Icon, labelKey, to }) => (
    <NavLink
      className={linkClass}
      end={end ?? false}
      key={labelKey}
      onClick={onClose}
      to={to}
    >
      <Icon aria-hidden="true" className="size-4" />
      {t(`nav.${labelKey}`)}
    </NavLink>
  ));
}
