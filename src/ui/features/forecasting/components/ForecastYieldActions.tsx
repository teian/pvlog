import { Button } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Forward forecast action properties. */
export interface ForecastYieldActionsProps {
  /** Starts an export of the forward forecast in the given format. */ onExport: (
    format: "csv" | "json",
  ) => void;
  /** Switches between chart and table presentation. */ onViewChange: (
    table: boolean,
  ) => void;
  /** Whether the table view is active. */ table: boolean;
}

/** Renders the chart/table switch and the CSV/JSON export triggers of the forward forecast. @param props - Current view and the view/export callbacks. @returns The action bar. */
export function ForecastYieldActions({
  onExport,
  onViewChange,
  table,
}: ForecastYieldActionsProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap gap-2">
      <Button
        aria-pressed={!table}
        onClick={() => {
          onViewChange(false);
        }}
        size="sm"
        variant={table ? "outline" : "default"}
      >
        {t("forecasting.view.chart")}
      </Button>
      <Button
        aria-pressed={table}
        onClick={() => {
          onViewChange(true);
        }}
        size="sm"
        variant={table ? "default" : "outline"}
      >
        {t("forecasting.view.table")}
      </Button>
      <Button
        onClick={() => {
          onExport("csv");
        }}
        size="sm"
        variant="outline"
      >
        {t("forecasting.exportCsv")}
      </Button>
      <Button
        onClick={() => {
          onExport("json");
        }}
        size="sm"
        variant="outline"
      >
        {t("forecasting.exportJson")}
      </Button>
    </div>
  );
}
