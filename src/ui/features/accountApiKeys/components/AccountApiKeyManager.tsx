import { AccountApiKeyCreateForm } from "@/features/accountApiKeys/components/AccountApiKeyCreateForm";
import { AccountApiKeyTable } from "@/features/accountApiKeys/components/AccountApiKeyTable";
import { OneTimeApiKeyDialog } from "@/features/accountApiKeys/components/OneTimeApiKeyDialog";
import {
  useAccountApiKeys,
  useCreateAccountApiKey,
} from "@/features/accountApiKeys/hooks/useAccountApiKeys";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Coordinates safe API-key metadata with an ephemeral one-time secret. @returns Account API-key management cards. */
export function AccountApiKeyManager() {
  const { t } = useTranslation();
  const keys = useAccountApiKeys();
  const [secret, setSecret] = useState<string | null>(null);
  const create = useCreateAccountApiKey((apiKey) => {
    setSecret(apiKey);
  });
  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>{t("accountApiKeys.createTitle")}</CardTitle>
          <CardDescription>
            {t("accountApiKeys.createDescription")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          <AccountApiKeyCreateForm
            pending={create.isPending}
            onSubmit={(input) => {
              create.mutate(input);
            }}
          />
          {create.isError ? (
            <p className="mt-3 text-sm text-destructive" role="alert">
              {t("accountApiKeys.createError")}
            </p>
          ) : null}
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>{t("accountApiKeys.listTitle")}</CardTitle>
          <CardDescription>
            {t("accountApiKeys.listDescription")}
          </CardDescription>
        </CardHeader>
        <CardContent>
          {keys.isPending ? <Skeleton className="h-24 w-full" /> : null}
          {keys.isError ? (
            <Alert variant="destructive">
              <AlertTitle>{t("accountApiKeys.loadErrorTitle")}</AlertTitle>
              <AlertDescription>
                {t("accountApiKeys.loadErrorDescription")}
              </AlertDescription>
            </Alert>
          ) : null}
          {keys.data ? <AccountApiKeyTable keys={keys.data} /> : null}
        </CardContent>
      </Card>
      <OneTimeApiKeyDialog
        apiKey={secret}
        onClose={() => {
          setSecret(null);
        }}
      />
    </>
  );
}
