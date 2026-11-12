import type { HistoryImport } from "@/entities/dataSource";
import { useTranslation } from "react-i18next";

/** Lists the imports committed earlier. @param props - Committed imports, newest first. @returns The list, or nothing when there are none. */
export function HistoryImportHistory({
  imports,
}: {
  imports: HistoryImport[];
}) {
  const { t, i18n } = useTranslation();
  if (imports.length === 0) return null;
  const time = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeStyle: "short",
  });
  return (
    <div className="flex flex-col gap-1 text-sm">
      <p className="font-medium">{t("dataSources.import.earlier")}</p>
      <ul className="flex flex-col gap-1 text-muted-foreground">
        {imports.map((item) => (
          <li key={item.id}>
            {t("dataSources.import.earlierItem", {
              file: item.fileName,
              at: time.format(
                item.committedAtEpochMillis ?? item.createdAtEpochMillis,
              ),
              stored: item.result?.stored ?? 0,
            })}
          </li>
        ))}
      </ul>
    </div>
  );
}
