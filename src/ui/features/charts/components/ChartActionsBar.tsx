import { Button, Toggle, ToggleGroup, ToggleGroupItem } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Chart view mode: rendered chart or the accessible data table. */
export type ChartView = "chart" | "table";

/** Chart actions bar properties. */
export interface ChartActionsBarProps {
  /** Currently selected view. */ view: ChartView;
  /** Invoked with the newly selected view. */
  onViewChange: (view: ChartView) => void;
  /** Whether the previous-period comparison is shown. */
  compareEnabled: boolean;
  /** Invoked with the new comparison toggle state. */
  onCompareChange: (enabled: boolean) => void;
  /** Invoked with the requested export format. */
  onExport: (format: "csv" | "json") => void;
  /** Whether an export request is in flight. */ exportPending: boolean;
}

/** Renders the per-chart view toggle, previous-period comparison toggle, and CSV/JSON export actions. @param props - Current state and change handlers. @returns The chart actions bar. */
export function ChartActionsBar({
  view,
  onViewChange,
  compareEnabled,
  onCompareChange,
  onExport,
  exportPending,
}: ChartActionsBarProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-wrap items-center gap-2">
      <ToggleGroup
        aria-label={t("charts.viewLabel")}
        onValueChange={(value: string) => {
          if (value) onViewChange(value as ChartView);
        }}
        size="sm"
        type="single"
        value={view}
        variant="outline"
      >
        <ToggleGroupItem value="chart">{t("charts.view.chart")}</ToggleGroupItem>
        <ToggleGroupItem value="table">{t("charts.view.table")}</ToggleGroupItem>
      </ToggleGroup>
      <Toggle
        onPressedChange={onCompareChange}
        pressed={compareEnabled}
        size="sm"
        variant="outline"
      >
        {t("charts.compareToggle")}
      </Toggle>
      <div className="ml-auto flex gap-2">
        <Button
          disabled={exportPending}
          onClick={() => {
            onExport("csv");
          }}
          size="sm"
          variant="outline"
        >
          {t("charts.export.csv")}
        </Button>
        <Button
          disabled={exportPending}
          onClick={() => {
            onExport("json");
          }}
          size="sm"
          variant="outline"
        >
          {t("charts.export.json")}
        </Button>
      </div>
    </div>
  );
}
