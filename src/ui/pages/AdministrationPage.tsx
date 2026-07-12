import {
  AuditPanel,
  ConnectorPanel,
  IdentityPanel,
  InvitationPanel,
  RolesPanel,
  SystemResourcesPanel,
  OperationsPanel,
} from "@/features/administration";
import { useSession } from "@/features/auth";
import { EquipmentCatalogPanel } from "@/features/equipmentCatalog";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";

/** Displays session identity links and account-scoped RBAC/audit data when authorized. @returns The administration page. */
export function AdministrationPage() {
  const { t } = useTranslation();
  const session = useSession();
  const accountId = session.data?.accountId;
  return (
    <AppShell accountId={accountId} systemIds={session.data?.systemIds}>
      <section aria-labelledby="administration-title" className="space-y-2">
        <h1 className="text-2xl font-semibold" id="administration-title">
          {t("administration.title")}
        </h1>
        <p className="text-muted-foreground">
          {t("administration.description")}
        </p>
      </section>
      <IdentityPanel />
      <InvitationPanel />
      <ConnectorPanel />
      <RolesPanel accountId={accountId} />
      <EquipmentCatalogPanel
        accountId={accountId}
        systemId={session.data?.systemIds[0]}
      />
      <SystemResourcesPanel
        accountId={accountId}
        systemId={session.data?.systemIds[0]}
      />
      <AuditPanel accountId={accountId} />
      <OperationsPanel accountId={accountId} />
    </AppShell>
  );
}
