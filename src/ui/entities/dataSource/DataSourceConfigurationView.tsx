import { CopyButton } from "@/entities/dataSource/CopyButton";
import type { DataSourceConfiguration } from "@/entities/dataSource/dataSource.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Input,
  Label,
} from "@/shared/components";
import { TriangleAlertIcon } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";

/** Shows a freshly issued uploader configuration that cannot be retrieved again. @param props - One-time configuration held in memory by the caller. @returns Server address, system identifier, key and ready-to-paste snippets, each with a copy action. */
export function DataSourceConfigurationView({
  configuration,
}: {
  configuration: DataSourceConfiguration;
}) {
  const { t } = useTranslation();
  const baseId = useId();
  const values = [
    { key: "serverUrl", value: configuration.serverUrl },
    { key: "systemId", value: configuration.systemId },
    { key: "apiKey", value: configuration.apiKey },
  ] as const;
  return (
    <div className="flex flex-col gap-4">
      <Alert>
        <TriangleAlertIcon />
        <AlertTitle>{t("dataSources.configuration.onceTitle")}</AlertTitle>
        <AlertDescription>
          {t("dataSources.configuration.onceDescription")}
        </AlertDescription>
      </Alert>
      <dl className="flex flex-col gap-3">
        {values.map(({ key, value }) => {
          const label = t(`dataSources.configuration.${key}`);
          return (
            <div className="flex flex-col gap-1.5" key={key}>
              <dt>
                <Label htmlFor={`${baseId}-${key}`}>{label}</Label>
              </dt>
              <dd className="flex gap-2">
                <Input
                  className="font-mono"
                  id={`${baseId}-${key}`}
                  readOnly
                  value={value}
                />
                <CopyButton label={label} value={value} />
              </dd>
            </div>
          );
        })}
      </dl>
      {configuration.snippets.map((snippet) => (
        <section
          aria-label={snippet.title}
          className="flex flex-col gap-1.5"
          key={snippet.id}
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-semibold">{snippet.title}</span>
            <CopyButton label={snippet.title} value={snippet.content} />
          </div>
          <pre className="overflow-x-auto rounded-md border bg-muted/40 p-3 font-mono text-xs">
            {snippet.content}
          </pre>
        </section>
      ))}
    </div>
  );
}
