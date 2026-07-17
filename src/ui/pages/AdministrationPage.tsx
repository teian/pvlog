import {
  AdministrationSectionContent,
  type AdministrationSection,
} from "@/features/administration";
import { useSession } from "@/features/auth";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";
import { useSearchParams } from "react-router";

const sections = new Set<AdministrationSection>([
  "users",
  "data-sources",
  "alert-rules",
  "notifications",
  "retention-backup",
  "system-logs",
]);

/** Displays session identity links and account-scoped RBAC/audit data when authorized. @returns The administration page. */
export function AdministrationPage() {
  const { t } = useTranslation();
  const session = useSession();
  const accountId = session.data?.accountId;
  const [searchParams] = useSearchParams();
  const requestedSection = searchParams.get("section") as AdministrationSection;
  const section = sections.has(requestedSection) ? requestedSection : "users";
  return (
    <AppShell systemIds={session.data?.systemIds} variant="administration">
      <section aria-labelledby="administration-title" className="space-y-2">
        <h1 className="text-2xl font-extrabold" id="administration-title">
          {t("administration.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("administration.description")}
        </p>
      </section>
      <AdministrationSectionContent
        accountId={accountId}
        section={section}
        systemId={session.data?.systemIds[0]}
      />
    </AppShell>
  );
}
