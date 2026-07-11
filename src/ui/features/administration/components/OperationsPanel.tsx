import { useOperationalSummary } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

const categories = [
  "alerts",
  "alertEvents",
  "webhooks",
  "importsExports",
  "workers",
  "storage",
  "backups",
  "readiness",
] as const;

/** Displays safe operational administration availability and counts. @param props - Active account context. @returns The operations panel. */
export function OperationsPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const summary = useOperationalSummary(accountId);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.operations.title")}</CardTitle>
        <CardDescription>
          {t("administration.operations.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {categories.map((category) => {
            const count = summary.data?.[category];
            return (
              <div className="rounded-md border p-3" key={category}>
                <dt className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
                  {t(`administration.operations.${category}`)}
                </dt>
                <dd className="mt-1 text-sm font-medium">
                  {count === undefined || count === null
                    ? t("administration.operations.unavailable")
                    : t("administration.operations.available", { count })}
                </dd>
              </div>
            );
          })}
        </dl>
      </CardContent>
    </Card>
  );
}
