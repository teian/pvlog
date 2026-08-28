import { Button } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Performance card action properties. */
export interface PerformanceCardActionsProps {
  /** Starts a CSV export of the current metric and range. */ onExport: () => void;
  /** Switches between chart and table presentation. */ onViewChange: (
    table: boolean,
  ) => void;
  /** Whether the table view is active. */ table: boolean;
}

/** Renders the chart/table switch and the CSV export trigger of a performance card. @param props - Current view and the view/export callbacks. @returns The card action bar. */
export function PerformanceCardActions({
  onExport,
  onViewChange,
  table,
}: PerformanceCardActionsProps) {
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
      <Button onClick={onExport} size="sm" variant="outline">
        {t("forecasting.exportCsv")}
      </Button>
    </div>
  );
}
