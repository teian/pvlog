import { AccountApiKeyManager } from "@/features/accountApiKeys";
import {
  AccountProfileForm,
  PasswordChangeForm,
} from "@/features/accountSettings";
import { useSession } from "@/features/auth";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";

/** Displays all self-service account and credential settings. @returns Consolidated account page. */
export function AccountPage() {
  const { t } = useTranslation();
  const session = useSession();
  const canManageApiKeys =
    session.data?.permissions.includes("credential_manage");
  return (
    <AppShell systemIds={session.data?.systemIds}>
      <header className="flex flex-col gap-2">
        <h1 className="text-2xl font-bold tracking-tight">
          {t("account.title")}
        </h1>
        <p className="text-sm text-muted-foreground">
          {t("account.description")}
        </p>
      </header>
      <div className="grid gap-6 xl:grid-cols-2">
        <AccountProfileForm />
        <PasswordChangeForm />
      </div>
      {canManageApiKeys ? (
        <section aria-labelledby="account-api-keys-title" className="space-y-6">
          <header className="space-y-2">
            <h2 className="text-xl font-semibold" id="account-api-keys-title">
              {t("accountApiKeys.title")}
            </h2>
            <p className="text-sm text-muted-foreground">
              {t("accountApiKeys.description")}
            </p>
          </header>
          <AccountApiKeyManager />
        </section>
      ) : null}
    </AppShell>
  );
}
