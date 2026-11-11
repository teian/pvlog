import { useSession } from "@/features/auth";
import { DataSourceManager } from "@/features/dataSources";
import { SystemEquipmentEditor } from "@/features/systemManagement";
import { SystemGeneralSettings } from "@/features/systemSettings";
import { cn } from "@/shared/lib/utils";
import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";
import { Navigate, NavLink, useParams } from "react-router";

/** Properties every settings section receives. */
interface SectionProps {
  systemId: string;
  /** True when the session may read the system but not manage it. */
  readOnly: boolean;
}

/**
 * The sections of a system's settings, in the order they are offered.
 *
 * Every per-system setting is edited in exactly one of them, and each has its
 * own address so a checklist or an empty state can link straight to it.
 */
const SECTIONS: { key: string; component: ComponentType<SectionProps> }[] = [
  { key: "general", component: SystemGeneralSettings },
  { key: "data-sources", component: DataSourceManager },
  { key: "equipment", component: SystemEquipmentEditor },
];

const linkClass = ({ isActive }: { isActive: boolean }) =>
  cn(
    "rounded-md px-3 py-2 text-sm transition-colors",
    isActive
      ? "bg-accent font-semibold text-accent-foreground"
      : "text-muted-foreground hover:bg-accent/60 hover:text-foreground",
  );

/** Shows one section of a system's settings with the navigation between them. @returns The settings area of a system. */
export function SystemSettingsPage() {
  const { t } = useTranslation();
  const session = useSession();
  const { systemId, section } = useParams();
  if (!systemId) return null;
  const current = SECTIONS.find(
    (entry) => entry.key === (section ?? "general"),
  );
  if (!current)
    return <Navigate replace to={`/systems/${systemId}/settings/general`} />;
  const Section = current.component;
  const readOnly = !session.data?.permissions.includes("system_manage");
  return (
    <div className="grid gap-6 md:grid-cols-[13rem_1fr]">
      <nav
        aria-label={t("systemSettings.sectionsLabel")}
        className="flex flex-row flex-wrap gap-1 md:flex-col"
      >
        {SECTIONS.map(({ key }) => (
          <NavLink
            className={linkClass}
            key={key}
            to={`/systems/${systemId}/settings/${key}`}
          >
            {t(`systemSettings.sections.${key}.title`)}
          </NavLink>
        ))}
      </nav>
      <section
        aria-labelledby="settings-section-title"
        className="flex min-w-0 flex-col gap-4"
      >
        <header className="flex flex-col gap-1">
          <h1 className="text-xl font-extrabold" id="settings-section-title">
            {t(`systemSettings.sections.${current.key}.title`)}
          </h1>
          <p className="text-sm text-muted-foreground">
            {t(`systemSettings.sections.${current.key}.description`)}
          </p>
          {readOnly ? (
            <p className="text-sm text-muted-foreground">
              {t("systemSettings.readOnly")}
            </p>
          ) : null}
        </header>
        <Section readOnly={readOnly} systemId={systemId} />
      </section>
    </div>
  );
}
