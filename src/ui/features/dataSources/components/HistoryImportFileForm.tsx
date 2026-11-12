import { MAXIMUM_IMPORT_FILE_BYTES } from "@/entities/dataSource";
import { Button, Input, Label } from "@/shared/components";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

/** Bytes in a mebibyte, the unit the limit is shown in. */
export const MEBIBYTE = 1024 * 1024;

/** Lets the person choose an export and start its dry run; a file over the limit is refused before it is sent. @param props - Whether a dry run is running, what to do when the choice changes, and how to start a dry run. @returns The file form. */
export function HistoryImportFileForm({
  pending,
  onChoose,
  onDryRun,
}: {
  pending: boolean;
  onChoose: () => void;
  onDryRun: (file: File) => void;
}) {
  const { t } = useTranslation();
  const inputId = useId();
  const [file, setFile] = useState<File | null>(null);
  const tooLarge = (file?.size ?? 0) > MAXIMUM_IMPORT_FILE_BYTES;
  return (
    <form
      className="flex flex-col gap-2"
      onSubmit={(event) => {
        event.preventDefault();
        if (file !== null && !tooLarge) onDryRun(file);
      }}
    >
      <div className="flex flex-wrap items-end gap-2">
        <div className="flex flex-col gap-1">
          <Label htmlFor={inputId}>{t("dataSources.import.file")}</Label>
          <Input
            accept=".csv,.txt,text/csv,text/plain"
            id={inputId}
            onChange={(event) => {
              setFile(event.target.files?.[0] ?? null);
              onChoose();
            }}
            type="file"
          />
        </div>
        <Button
          disabled={file === null || tooLarge || pending}
          type="submit"
          variant="outline"
        >
          {t("dataSources.import.dryRun")}
        </Button>
      </div>
      {tooLarge ? (
        <p className="text-sm text-destructive" role="alert">
          {t("dataSources.import.errors.tooLarge", {
            limit: MAXIMUM_IMPORT_FILE_BYTES / MEBIBYTE,
          })}
        </p>
      ) : null}
    </form>
  );
}
