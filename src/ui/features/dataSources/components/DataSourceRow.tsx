/* eslint-disable max-lines-per-function -- one row owns the rename field and the three confirmed actions of its source */
import type { DataSource } from "@/entities/dataSource";
import { DataSourceConfirmButton } from "@/features/dataSources/components/DataSourceConfirmButton";
import { DataSourceStateBadge } from "@/features/dataSources/components/DataSourceStateBadge";
import { Button, Input, TableCell, TableRow } from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Actions a row can ask its list to perform. */
export interface DataSourceRowActions {
  rename: (id: string, name: string) => void;
  regenerate: (id: string) => void;
  remove: (id: string) => void;
  sendTest: (id: string) => void;
}

/** Shows one data source with its state and, when allowed, its actions. @param props - The source, whether it may only be viewed, whether a request is running, and the actions. @returns A table row. */
export function DataSourceRow({
  dataSource,
  readOnly,
  busy,
  actions,
}: {
  dataSource: DataSource;
  readOnly: boolean;
  busy: boolean;
  actions: DataSourceRowActions;
}) {
  const { t, i18n } = useTranslation();
  const [name, setName] = useState<string>();
  const time = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "short",
  });
  const profile = t(`dataSources.profiles.${dataSource.profile}.name`, {
    defaultValue: dataSource.profile,
  });
  return (
    <TableRow>
      <TableCell className="font-medium">
        {name === undefined ? (
          dataSource.name
        ) : (
          <form
            className="flex gap-2"
            onSubmit={(event) => {
              event.preventDefault();
              if (name.trim()) actions.rename(dataSource.id, name.trim());
              setName(undefined);
            }}
          >
            <Input
              aria-label={t("dataSources.name")}
              onChange={(event) => {
                setName(event.target.value);
              }}
              value={name}
            />
            <Button size="sm" type="submit">
              {t("dataSources.saveName")}
            </Button>
          </form>
        )}
      </TableCell>
      <TableCell>{profile}</TableCell>
      <TableCell>
        <DataSourceStateBadge state={dataSource.state} />
        {dataSource.state === "stale" ? (
          <a className="ml-2 text-xs underline" href="/docs/api">
            {t("dataSources.troubleshooting")}
          </a>
        ) : null}
      </TableCell>
      <TableCell>
        {dataSource.lastReadingAtEpochMillis === null
          ? t("dataSources.noReading")
          : time.format(dataSource.lastReadingAtEpochMillis)}
      </TableCell>
      {readOnly ? null : (
        <TableCell>
          <div className="flex flex-wrap gap-2">
            <Button
              disabled={busy}
              onClick={() => {
                setName(dataSource.name);
              }}
              size="sm"
              variant="outline"
            >
              {t("dataSources.rename")}
            </Button>
            <Button
              disabled={busy}
              onClick={() => {
                actions.sendTest(dataSource.id);
              }}
              size="sm"
              variant="outline"
            >
              {t("setup.verify.sendTest")}
            </Button>
            {dataSource.profile === "import" ? null : (
              <DataSourceConfirmButton
                confirmLabel={t("dataSources.regenerate.confirm")}
                description={t("dataSources.regenerate.description", {
                  name: dataSource.name,
                })}
                disabled={busy}
                label={t("dataSources.regenerate.action")}
                onConfirm={() => {
                  actions.regenerate(dataSource.id);
                }}
                title={t("dataSources.regenerate.title")}
              />
            )}
            <DataSourceConfirmButton
              confirmLabel={t("dataSources.delete.confirm")}
              description={t("dataSources.delete.description", {
                name: dataSource.name,
              })}
              disabled={busy}
              label={t("dataSources.delete.action")}
              onConfirm={() => {
                actions.remove(dataSource.id);
              }}
              title={t("dataSources.delete.title")}
            />
          </div>
        </TableCell>
      )}
    </TableRow>
  );
}
