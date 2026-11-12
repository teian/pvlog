import { useSetupStatus } from "@/entities/system/useSystem";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/**
 * Says that no reading has arrived yet and names the next step, in place of
 * values that would look like measurements.
 *
 * @param props - The system the view describes; omitted when the view spans several systems.
 * @returns The no-data state with a link to where the next step is done.
 */
export function NoDataNotice({ systemId }: { systemId?: string | undefined }) {
  const { t } = useTranslation();
  const status = useSetupStatus(systemId ?? "");
  const hasSource =
    status.data?.items.find((item) => item.key === "data_source")?.state ===
    "done";
  const step = !systemId ? "systems" : hasSource ? "waiting" : "connect";
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <h2>{t("system.noData.title")}</h2>
        </CardTitle>
        <CardDescription>
          {t(`system.noData.${step}.description`)}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Button asChild>
          <Link
            to={
              systemId
                ? `/systems/${systemId}/settings/data-sources`
                : "/systems"
            }
          >
            {t(`system.noData.${step}.action`)}
          </Link>
        </Button>
      </CardContent>
    </Card>
  );
}
