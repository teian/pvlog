import { EquipmentCatalogPanel } from "@/features/equipmentCatalog";
import { AlertRulesPanel } from "./AlertRulesPanel";
import { AuditPanel } from "./AuditPanel";
import { ConnectorPanel } from "./ConnectorPanel";
import { EmailNotificationPanel } from "./EmailNotificationPanel";
import { NotificationChannelsPanel } from "./NotificationChannelsPanel";
import { OperationsPanel } from "./OperationsPanel";
import { RetentionBackupPanel } from "./RetentionBackupPanel";
import { SystemResourcesPanel } from "./SystemResourcesPanel";
import { UsersRolesPanel } from "./UsersRolesPanel";
import { WeatherFeedPanel } from "./WeatherFeedPanel";

/** Supported administration navigation sections. */
export type AdministrationSection =
  | "users"
  | "data-sources"
  | "alert-rules"
  | "notifications"
  | "retention-backup"
  | "system-logs";

/** Renders only the administration section selected in the dedicated sidebar. */
export function AdministrationSectionContent({
  accountId,
  section,
  systemId,
}: {
  accountId: string | null | undefined;
  section: AdministrationSection;
  systemId: string | undefined;
}) {
  if (section === "users") return <UsersRolesPanel accountId={accountId} />;
  if (section === "data-sources")
    return (
      <>
        <WeatherFeedPanel />
        <ConnectorPanel />
        <SystemResourcesPanel accountId={accountId} systemId={systemId} />
        <EquipmentCatalogPanel accountId={accountId} systemId={systemId} />
      </>
    );
  if (section === "alert-rules")
    return <AlertRulesPanel accountId={accountId} />;
  if (section === "notifications")
    return (
      <>
        <EmailNotificationPanel />
        <NotificationChannelsPanel accountId={accountId} />
      </>
    );
  if (section === "retention-backup")
    return (
      <>
        <RetentionBackupPanel />
        <OperationsPanel accountId={accountId} />
      </>
    );
  return <AuditPanel accountId={accountId} />;
}
