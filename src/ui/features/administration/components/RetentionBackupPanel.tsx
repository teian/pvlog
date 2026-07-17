import { useRetentionBackupSettings } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { RetentionBackupSettingsForm } from "./RetentionBackupSettingsForm";

/** Displays persisted retention and backup controls. */
export function RetentionBackupPanel() {
  const { t } = useTranslation();
  const query = useRetentionBackupSettings();
  return (
    <Card>
      <CardHeader className="border-b">
        <CardTitle>{t("administration.retention.title")}</CardTitle>
        <CardDescription>
          {t("administration.retention.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {query.isLoading ? <Skeleton className="h-28" /> : null}
        {query.data ? (
          <RetentionBackupSettingsForm
            initial={query.data}
            key={query.data.updatedAtEpochMillis}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}
