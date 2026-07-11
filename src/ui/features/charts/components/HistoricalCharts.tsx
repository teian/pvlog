import type { ResolutionParam } from "@/features/charts/api/chartsApi";
import { CategoryChart } from "@/features/charts/components/CategoryChart";
import { ChartControls } from "@/features/charts/components/ChartControls";
import {
  CHART_CATEGORIES,
  type ChartCategoryKey,
} from "@/features/charts/utils/chartCategories";
import {
  RANGE_PRESETS,
  type RangePresetKey,
} from "@/features/charts/utils/rangePresets";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

const MAXIMUM_POINTS = 720;
const DEFAULT_CATEGORIES: ChartCategoryKey[] = ["generation", "consumption"];

/** Historical charts panel properties. */
export interface HistoricalChartsProps {
  /** System whose telemetry is charted. */ systemId: string;
}

/** Renders shared range/resolution/category controls plus one bounded chart per active category. @param props - Target system. @returns The historical charts panel. */
export function HistoricalCharts({ systemId }: HistoricalChartsProps) {
  const { t } = useTranslation();
  const timezone = useMemo(
    () => Intl.DateTimeFormat().resolvedOptions().timeZone,
    [],
  );
  const [rangeKey, setRangeKey] = useState<RangePresetKey>("week");
  const [resolution, setResolution] = useState<ResolutionParam>("auto");
  const [activeCategories, setActiveCategories] =
    useState<ChartCategoryKey[]>(DEFAULT_CATEGORIES);
  const preset =
    RANGE_PRESETS.find((candidate) => candidate.key === rangeKey) ??
    RANGE_PRESETS[0];
  const visibleCategories = CHART_CATEGORIES.filter((category) =>
    activeCategories.includes(category.key),
  );

  return (
    <div className="flex flex-col gap-6">
      <ChartControls
        activeCategories={activeCategories}
        onCategoriesChange={setActiveCategories}
        onRangeChange={setRangeKey}
        onResolutionChange={setResolution}
        rangeKey={rangeKey}
        resolution={resolution}
      />
      {visibleCategories.length === 0 ? (
        <p className="text-sm text-muted-foreground">
          {t("charts.noCategories")}
        </p>
      ) : (
        <div className="grid gap-4 lg:grid-cols-2">
          {visibleCategories.map((category) => (
            <CategoryChart
              category={category}
              durationMillis={preset.durationMillis}
              key={category.key}
              maximumPoints={MAXIMUM_POINTS}
              resolution={resolution}
              systemId={systemId}
              timezone={timezone}
            />
          ))}
        </div>
      )}
    </div>
  );
}
