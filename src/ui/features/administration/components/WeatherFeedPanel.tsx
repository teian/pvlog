import { useWeatherFeedSettings } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { WeatherFeedSettingsForm } from "./WeatherFeedSettingsForm";

/** Displays provider-neutral site weather feed settings. */
export function WeatherFeedPanel() {
  const { t } = useTranslation();
  const query = useWeatherFeedSettings();
  return (
    <Card>
      <CardHeader className="border-b">
        <CardTitle>{t("administration.weatherFeed.title")}</CardTitle>
        <CardDescription>
          {t("administration.weatherFeed.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {query.isLoading ? <Skeleton className="h-24" /> : null}
        {query.data ? (
          <WeatherFeedSettingsForm
            initial={query.data}
            key={query.data.updatedAtEpochMillis}
          />
        ) : null}
      </CardContent>
    </Card>
  );
}
