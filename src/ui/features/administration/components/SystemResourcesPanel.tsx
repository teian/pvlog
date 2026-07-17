import { useSystemResources } from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Summarizes system-owned aggregate and account resources. @param props - Active account and system identifiers. @returns The resource administration panel. */
export function SystemResourcesPanel({
  accountId,
  systemId,
}: {
  accountId: string | null | undefined;
  systemId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const resources = useSystemResources(accountId, systemId);
  const counts = [
    ["inverters", resources.inverters.data?.length],
    ["equipment", resources.equipment.data?.length],
    ["tariffs", resources.tariffs.data?.length],
    ["channels", resources.channels.data?.length],
    ["memberships", resources.memberships.data?.length],
    ["credentials", resources.credentials.data?.length],
  ] as const;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.resources.title")}</CardTitle>
        <CardDescription>
          {t("administration.resources.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {!systemId ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.resources.noSystem")}
          </p>
        ) : null}
        <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {counts.map(([kind, count]) => (
            <div className="rounded-md border p-3" key={kind}>
              <dt className="text-xs font-semibold uppercase tracking-widest text-muted-foreground">
                {t(`administration.resources.${kind}`)}
              </dt>
              <dd className="mt-1 text-2xl font-bold tabular-nums">
                {count ?? "—"}
              </dd>
            </div>
          ))}
        </dl>
        {resources.inverters.data?.map((inverter) => (
          <article className="rounded-md border p-4" key={inverter.id}>
            <h3 className="font-medium">{inverter.name}</h3>
            <p className="text-sm text-muted-foreground">
              {t("administration.resources.stringCount", {
                count: inverter.strings.length,
              })}
            </p>
            <ul
              className="mt-2 space-y-1"
              aria-label={t("administration.resources.strings")}
            >
              {inverter.strings.map((string) => (
                <li className="text-sm" key={string.id}>
                  {string.name}
                  {" · "}
                  {t("administration.resources.panels", {
                    count: string.panelCount,
                  })}
                </li>
              ))}
            </ul>
          </article>
        ))}
      </CardContent>
    </Card>
  );
}
