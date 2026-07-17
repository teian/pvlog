import { useSession } from "@/features/auth";
import { SystemManagementView } from "@/features/systemManagement";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";

/** Renders system management and its single-page create/edit wizard. @returns Protected management route. */
export function OnboardingPage() {
  const { t } = useTranslation();
  const session = useSession();
  return (
    <AppShell systemIds={session.data?.systemIds}>
      <header className="flex flex-col gap-1">
        <h1 className="text-2xl font-extrabold tracking-tight">
          {t("systemManagement.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("systemManagement.subtitle", {
            count: session.data?.systemIds.length ?? 0,
          })}
        </p>
      </header>
      <SystemManagementView />
    </AppShell>
  );
}
