import { useDataSources } from "@/entities/dataSource";
import { useSystem } from "@/entities/system";
import { Badge } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Names the system and data source a managed API key belongs to. @param props - Identifiers carried by the key. @returns The data source and system names with a link to where the key is managed. */
export function ManagedApiKeyName({
  systemId,
  dataSourceId,
}: {
  systemId: string;
  dataSourceId: string;
}) {
  const { t } = useTranslation();
  const system = useSystem(systemId);
  const sources = useDataSources(systemId);
  const source = sources.data?.find((entry) => entry.id === dataSourceId);
  return (
    <span className="flex flex-col gap-1">
      <span className="flex flex-wrap items-center gap-2">
        {source?.name ?? t("accountApiKeys.managed.unknownSource")}
        <Badge variant="outline">{t("accountApiKeys.managed.badge")}</Badge>
      </span>
      <span className="text-xs font-normal text-muted-foreground">
        {t("accountApiKeys.managed.system", {
          name: system.data?.name ?? t("accountApiKeys.managed.unknownSystem"),
        })}{" "}
        <a
          className="underline"
          href={`/systems/${systemId}/settings/data-sources`}
        >
          {t("accountApiKeys.managed.link")}
        </a>
      </span>
    </span>
  );
}
