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

/** Converts an extensible backend identifier into an i18next-safe leaf key. */
function translationSegment(value: string): string {
  return value.replaceAll(".", "_");
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
    <Card className="gap-0 overflow-hidden py-0">
      <CardHeader className="border-b py-5">
        <CardTitle>{t("administration.audit.title")}</CardTitle>
        <CardDescription>
          {t("administration.audit.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="px-0">
        {audit.isLoading ? <Skeleton className="m-5 h-16" /> : null}
        {audit.isError ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {audit.data?.length === 0 ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.audit.empty")}
          </p>
        ) : null}
        <ul aria-label={t("administration.audit.title")}>
          {audit.data?.map((event) => (
            <li
              className="relative border-b py-3 pl-9 pr-5 last:border-b-0"
              key={event.id}
            >
              <span
                aria-hidden="true"
                className="absolute left-5 top-[1.1rem] size-2 rounded-full bg-primary"
              />
              <p className="font-mono text-xs text-muted-foreground">
                {formatTimestamp(event.occurredAt, i18n.language)}
                {" · "}
                {t(`administration.audit.actors.${event.actorType}`, {
                  defaultValue: event.actorType,
                })}
              </p>
              <p className="mt-1 text-sm font-semibold">
                {t(
                  `administration.audit.actions.${translationSegment(event.action)}`,
                  { defaultValue: event.action },
                )}
              </p>
              <p className="text-sm text-muted-foreground">
                {t("administration.audit.event", {
                  target: t(
                    `administration.audit.targets.${translationSegment(event.targetType)}`,
                    { defaultValue: event.targetType },
                  ),
                  outcome: t(
                    `administration.audit.outcomes.${translationSegment(event.outcome)}`,
                    { defaultValue: event.outcome },
                  ),
                })}
              </p>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
