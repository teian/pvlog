import { mayOpen, useSession, type GuardedDestination } from "@/features/auth";
import { cn } from "@/shared/lib/utils";
import {
  BarChart3Icon,
  CloudSunIcon,
  LayoutDashboardIcon,
  ListIcon,
  SunIcon,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { NavLink } from "react-router";

interface ViewNavigationItem {
  /** Destination whose permission the entry needs; omitted when signing in suffices. */
  destination?: GuardedDestination;
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

/** Renders the application views the session may open. @param props - Drawer close callback. @returns Localized view navigation without entries that would lead to a forbidden page. */
export function ApplicationViewNavigation({
  onClose,
}: {
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const permissions = useSession().data?.permissions;
  const items: ViewNavigationItem[] = [
    { end: true, icon: LayoutDashboardIcon, labelKey: "dashboard", to: "/" },
    {
      destination: "systems",
      end: true,
      icon: ListIcon,
      labelKey: "systems",
      to: "/systems",
    },
    {
      destination: "reports",
      icon: BarChart3Icon,
      labelKey: "statistics",
      to: "/statistics",
    },
    {
      destination: "reports",
      icon: SunIcon,
      labelKey: "seasonal",
      to: "/seasonal",
    },
    {
      destination: "reports",
      icon: CloudSunIcon,
      labelKey: "weather",
      to: "/weather",
    },
  ];

  return items
    .filter(
      ({ destination }) => !destination || mayOpen(permissions, destination),
    )
    .map(({ end, icon: Icon, labelKey, to }) => (
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
