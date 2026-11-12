import {
  MAXIMUM_IMPORT_FILE_BYTES,
  useCommitHistoryImport,
  useHistoryImports,
  useUploadHistoryImport,
  type DataSource,
} from "@/entities/dataSource";
import {
  HistoryImportFileForm,
  MEBIBYTE,
} from "@/features/dataSources/components/HistoryImportFileForm";
import { HistoryImportHistory } from "@/features/dataSources/components/HistoryImportHistory";
import { HistoryImportReportView } from "@/features/dataSources/components/HistoryImportReportView";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

/** Explains why a file was not read. @param t - Translator. @param error - Failure of the upload. @returns A sentence for the person who chose the file. */
function uploadError(t: TFunction, error: Error): string {
  if (!(error instanceof SessionRequestError))
    return t("dataSources.import.errors.failed");
  if (error.status === 413)
    return t("dataSources.import.errors.tooLarge", {
      limit: MAXIMUM_IMPORT_FILE_BYTES / MEBIBYTE,
    });
  return error.status === 422
    ? t("dataSources.import.errors.unreadable")
    : t("dataSources.import.errors.failed");
}

/** Imports PVOutput exports into an import source: a dry run shows what the file holds, and only an explicit commit stores it. @param props - The owning system and the import source. @returns A card with the import flow. */
export function HistoryImportPanel({
  systemId,
  dataSource,
}: {
  systemId: string;
  dataSource: DataSource;
}) {
  const { t } = useTranslation();
  const imports = useHistoryImports(systemId, dataSource.id);
  const upload = useUploadHistoryImport(systemId, dataSource.id);
  const commit = useCommitHistoryImport(systemId, dataSource.id);
  const current = commit.data ?? upload.data;
  const earlier = (imports.data ?? []).filter(
    (item) => item.state === "committed" && item.id !== current?.id,
  );
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          {t("dataSources.import.title", { name: dataSource.name })}
        </CardTitle>
        <CardDescription>{t("dataSources.import.description")}</CardDescription>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <HistoryImportFileForm
          onChoose={() => {
            upload.reset();
            commit.reset();
          }}
          onDryRun={(file) => {
            commit.reset();
            upload.mutate(file);
          }}
          pending={upload.isPending}
        />
        {upload.isPending ? (
          <p className="text-sm text-muted-foreground" role="status">
            {t("dataSources.import.reading")}
          </p>
        ) : null}
        {upload.error ? (
          <p className="text-sm text-destructive" role="alert">
            {uploadError(t, upload.error)}
          </p>
        ) : null}
        {current ? (
          <section
            aria-label={t("dataSources.import.reportTitle")}
            className="flex flex-col gap-3"
          >
            <p className="text-sm font-medium" role="status">
              {t(`dataSources.import.${current.state}`, {
                file: current.fileName,
              })}
            </p>
            <HistoryImportReportView historyImport={current} />
            {current.state === "ready" ? (
              <div className="flex flex-wrap items-center gap-2">
                <Button
                  disabled={current.report.accepted === 0 || commit.isPending}
                  onClick={() => {
                    commit.mutate(current.id);
                  }}
                >
                  {t("dataSources.import.commit", {
                    count: current.report.accepted,
                  })}
                </Button>
                {commit.isPending ? (
                  <span className="text-sm text-muted-foreground" role="status">
                    {t("dataSources.import.committing")}
                  </span>
                ) : null}
              </div>
            ) : null}
            {commit.isError ? (
              <p className="text-sm text-destructive" role="alert">
                {t("dataSources.import.errors.commit")}
              </p>
            ) : null}
          </section>
        ) : null}
        <HistoryImportHistory imports={earlier} />
      </CardContent>
    </Card>
  );
}
