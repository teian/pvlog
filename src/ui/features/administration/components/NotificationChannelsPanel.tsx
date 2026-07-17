import { useWebhooks } from "@/features/administration/hooks/useAdministration";
import {
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Displays the account notification channels supported by the current API. */
export function NotificationChannelsPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const webhooks = useWebhooks(accountId);

  return (
    <Card className="gap-0 overflow-hidden py-0">
      <CardHeader className="border-b py-5">
        <CardTitle>{t("administration.notifications.title")}</CardTitle>
        <CardDescription>
          {t("administration.notifications.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="px-0">
        {webhooks.isLoading ? <Skeleton className="m-5 h-20" /> : null}
        {webhooks.isError ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {webhooks.data?.length === 0 ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.notifications.empty")}
          </p>
        ) : null}
        <ul aria-label={t("administration.notifications.title")}>
          {webhooks.data?.map((webhook) => (
            <li
              className="flex flex-wrap items-center gap-3 border-b px-5 py-4 last:border-b-0"
              key={webhook.id}
            >
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-semibold">
                  {webhook.endpoint}
                </p>
                <p className="text-sm text-muted-foreground">
                  {t("administration.notifications.events", {
                    count: webhook.events.length,
                  })}
                </p>
              </div>
              <Badge
                variant={webhook.state === "active" ? "secondary" : "outline"}
              >
                {t(`administration.notifications.states.${webhook.state}`)}
              </Badge>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
