import { useRevokeAccountApiKey } from "@/features/accountApiKeys/hooks/useAccountApiKeys";
import { AccountApiKeyRow } from "@/features/accountApiKeys/components/AccountApiKeyRow";
import type { AccountApiKey } from "@/features/accountApiKeys/types/accountApiKeys.types";
import {
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Lists safe API-key metadata and independently confirmed revoke actions. @param props - Current account key metadata. @returns Responsive API-key table. */
export function AccountApiKeyTable({ keys }: { keys: AccountApiKey[] }) {
  const { i18n, t } = useTranslation();
  const revoke = useRevokeAccountApiKey();
  const formatDate = (value: number | null) =>
    value === null
      ? t("accountApiKeys.never")
      : new Intl.DateTimeFormat(i18n.language, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(value);
  if (keys.length === 0)
    return (
      <>
        <p className="text-sm text-muted-foreground">
          {t("accountApiKeys.empty")}
        </p>
        {revoke.isSuccess ? (
          <p aria-live="polite" className="mt-3 text-sm">
            {t("accountApiKeys.revokeSuccess")}
          </p>
        ) : null}
      </>
    );
  return (
    <>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>{t("accountApiKeys.table.name")}</TableHead>
            <TableHead>{t("accountApiKeys.table.permissions")}</TableHead>
            <TableHead>{t("accountApiKeys.table.created")}</TableHead>
            <TableHead>{t("accountApiKeys.table.expires")}</TableHead>
            <TableHead>{t("accountApiKeys.table.status")}</TableHead>
            <TableHead>
              <span className="sr-only">
                {t("accountApiKeys.table.actions")}
              </span>
            </TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {keys.map((key) => (
            <AccountApiKeyRow
              apiKey={key}
              formatDate={formatDate}
              key={key.id}
              onRevoke={(id) => {
                revoke.mutate(id);
              }}
            />
          ))}
        </TableBody>
      </Table>
      {revoke.isSuccess ? (
        <p aria-live="polite" className="mt-3 text-sm">
          {t("accountApiKeys.revokeSuccess")}
        </p>
      ) : null}
      {revoke.isError ? (
        <p className="mt-3 text-sm text-destructive" role="alert">
          {t("accountApiKeys.revokeError")}
        </p>
      ) : null}
    </>
  );
}
