import { DataQualityView } from "@/features/dataQuality";
import { useTranslation } from "react-i18next";
import { useParams } from "react-router";

/** Displays data-quality inspection and correction tools for one system. @returns The data quality tab. */
export function DataQualityPage() {
  const { t } = useTranslation();
  const { systemId } = useParams<{ systemId: string }>();

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-bold tracking-tight">
        {t("dataQuality.title")}
      </h1>
      {systemId ? <DataQualityView systemId={systemId} /> : null}
    </div>
  );
}
