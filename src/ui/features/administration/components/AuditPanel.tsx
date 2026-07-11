import { useAuditEvents } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

function formatTimestamp(epochMillis: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(epochMillis));
}

/** Renders server-filtered audit events when the active role permits them. @param props - Active account context. @returns The audit panel. */
export function AuditPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { i18n, t } = useTranslation();
  const audit = useAuditEvents(accountId);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.audit.title")}</CardTitle>
        <CardDescription>
          {t("administration.audit.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {audit.isLoading ? <Skeleton className="h-16 w-full" /> : null}
        {audit.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {audit.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.audit.empty")}
          </p>
        ) : null}
        <ul className="space-y-3" aria-label={t("administration.audit.title")}>
          {audit.data?.map((event) => (
            <li className="rounded-md border p-3" key={event.id}>
              <p className="font-medium">{event.action}</p>
              <p className="text-sm text-muted-foreground">
                {t("administration.audit.event", {
                  target: event.targetType,
                  outcome: event.outcome,
                  date: formatTimestamp(event.occurredAt, i18n.language),
                })}
              </p>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
