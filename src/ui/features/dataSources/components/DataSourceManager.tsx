/* eslint-disable max-lines-per-function -- the manager wires one list to its five mutations and the one-time dialog */
import {
  DataSourceConfigurationView,
  useCreateDataSource,
  useDataSources,
  useDeleteDataSource,
  useRegenerateDataSourceCredential,
  useRenameDataSource,
  useSendTestReading,
  useSourceProfiles,
  type DataSourceConfiguration,
} from "@/entities/dataSource";
import { DataSourceCreateForm } from "@/features/dataSources/components/DataSourceCreateForm";
import { DataSourceRow } from "@/features/dataSources/components/DataSourceRow";
import { HistoryImportPanel } from "@/features/dataSources/components/HistoryImportPanel";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Skeleton,
  Table,
  TableBody,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** How often the list refreshes the connection state of its sources. */
const STATE_REFRESH_INTERVAL_MILLIS = 30_000;

/** Lists a system's data sources and manages them. @param props - The system and whether its sources may only be viewed. @returns The data sources section of the system settings. */
export function DataSourceManager({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const sources = useDataSources(systemId, STATE_REFRESH_INTERVAL_MILLIS);
  const profiles = useSourceProfiles();
  // The key lives in this component's memory only and is dropped on close.
  const [configuration, setConfiguration] =
    useState<DataSourceConfiguration | null>(null);
  const show = (_: unknown, issued: DataSourceConfiguration | null) => {
    setConfiguration(issued);
  };
  const create = useCreateDataSource(systemId, show);
  const regenerate = useRegenerateDataSourceCredential(systemId, show);
  const rename = useRenameDataSource(systemId);
  const remove = useDeleteDataSource(systemId);
  const test = useSendTestReading(systemId);
  const busy =
    create.isPending ||
    regenerate.isPending ||
    rename.isPending ||
    remove.isPending ||
    test.isPending;
  const failed =
    regenerate.isError || rename.isError || remove.isError || test.isError;

  if (sources.isPending) return <Skeleton className="h-40 w-full" />;
  if (sources.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("dataSources.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("dataSources.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-muted-foreground">
        {t("dataSources.description")}
      </p>
      {sources.data.length === 0 ? (
        <Alert>
          <AlertTitle>{t("dataSources.empty.title")}</AlertTitle>
          <AlertDescription>
            {t("dataSources.empty.description")}
          </AlertDescription>
        </Alert>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("dataSources.columns.name")}</TableHead>
              <TableHead>{t("dataSources.columns.profile")}</TableHead>
              <TableHead>{t("dataSources.columns.state")}</TableHead>
              <TableHead>{t("dataSources.columns.lastReading")}</TableHead>
              {readOnly ? null : (
                <TableHead>{t("dataSources.columns.actions")}</TableHead>
              )}
            </TableRow>
          </TableHeader>
          <TableBody>
            {sources.data.map((dataSource) => (
              <DataSourceRow
                actions={{
                  rename: (id, name) => {
                    rename.mutate({ id, name });
                  },
                  regenerate: (id) => {
                    regenerate.mutate(id);
                  },
                  remove: (id) => {
                    remove.mutate(id);
                  },
                  sendTest: (id) => {
                    test.mutate(id);
                  },
                }}
                busy={busy}
                dataSource={dataSource}
                key={dataSource.id}
                readOnly={readOnly}
              />
            ))}
          </TableBody>
        </Table>
      )}
      {sources.data
        .filter((dataSource) => !readOnly && dataSource.profile === "import")
        .map((dataSource) => (
          <HistoryImportPanel
            dataSource={dataSource}
            key={dataSource.id}
            systemId={systemId}
          />
        ))}
      {failed ? (
        <p className="text-sm text-destructive" role="alert">
          {t("dataSources.errors.actionDescription")}
        </p>
      ) : null}
      {test.isSuccess ? (
        <p className="text-sm text-muted-foreground" role="status">
          {t("dataSources.testSent")}
        </p>
      ) : null}
      {readOnly || !profiles.data ? null : (
        <DataSourceCreateForm
          failed={create.isError}
          onSubmit={(input) => {
            create.mutate(input);
          }}
          pending={create.isPending}
          profiles={profiles.data}
        />
      )}
      <Dialog
        onOpenChange={(open) => {
          if (!open) setConfiguration(null);
        }}
        open={configuration !== null}
      >
        <DialogContent closeLabel={t("dataSources.configuration.close")}>
          <DialogHeader>
            <DialogTitle>{t("dataSources.configuration.title")}</DialogTitle>
            <DialogDescription>
              {t("dataSources.configuration.dialogDescription")}
            </DialogDescription>
          </DialogHeader>
          {configuration ? (
            <DataSourceConfigurationView configuration={configuration} />
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  );
}
