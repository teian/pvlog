import { useConnectors } from "@/features/administration/hooks/useAdministration";
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

/** Renders non-secret connector metadata when the browser session has instance-admin access. @returns The connector administration panel. */
export function ConnectorPanel() {
  const { t } = useTranslation();
  const connectors = useConnectors();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.connectors.title")}</CardTitle>
        <CardDescription>
          {t("administration.connectors.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {connectors.isLoading ? <Skeleton className="h-16 w-full" /> : null}
        {connectors.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {connectors.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.connectors.empty")}
          </p>
        ) : null}
        <ul
          className="space-y-3"
          aria-label={t("administration.connectors.title")}
        >
          {connectors.data?.map((connector) => (
            <li className="rounded-md border p-3" key={connector.id}>
              <div className="flex flex-wrap items-center gap-2">
                <p className="font-medium">{connector.displayName}</p>
                <Badge variant="secondary">
                  {t(
                    `administration.connectors.protocols.${connector.protocol}`,
                    { defaultValue: connector.protocol },
                  )}
                </Badge>
                <Badge variant={connector.enabled ? "default" : "outline"}>
                  {connector.enabled
                    ? t("administration.connectors.enabled")
                    : t("administration.connectors.disabled")}
                </Badge>
              </div>
              <p className="mt-1 text-sm text-muted-foreground">
                {t("administration.connectors.scopes", {
                  count: connector.scopes.length,
                })}
              </p>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
