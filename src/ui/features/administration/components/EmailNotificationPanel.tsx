import { useEmailNotificationSettings } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { EmailNotificationSettingsForm } from "./EmailNotificationSettingsForm";

/** Displays deployment-safe email delivery settings. */
export function EmailNotificationPanel() {
  const { t } = useTranslation();
  const query = useEmailNotificationSettings();
  return (
    <Card>
      <CardHeader className="border-b">
        <CardTitle>{t("administration.email.title")}</CardTitle>
        <CardDescription>
          {t("administration.email.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {query.isLoading ? <Skeleton className="h-36" /> : null}
        {query.data ? (
          <EmailNotificationSettingsForm
            initial={query.data}
            key={query.data.updatedAtEpochMillis}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}
