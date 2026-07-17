import type { ResolutionParam } from "@/features/charts/api/chartsApi";
import type { ChartCategoryKey } from "@/features/charts/utils/chartCategories";
import { ToggleGroup, ToggleGroupItem } from "@/shared/components";
import { RANGE_PRESETS, type RangePresetKey } from "@/shared/lib";
import { useTranslation } from "react-i18next";

const RESOLUTIONS: ResolutionParam[] = [
  "auto",
  "raw",
  "15m",
  "hour",
  "day",
  "month",
  "year",
];

const CATEGORIES: ChartCategoryKey[] = [
  "generation",
  "consumption",
  "grid",
  "battery",
  "environment",
  "extended",
];

/** Historical chart control properties. */
export interface ChartControlsProps {
  /** Currently selected range preset. */ rangeKey: RangePresetKey;
  /** Invoked with the newly selected range preset. */
  onRangeChange: (key: RangePresetKey) => void;
  /** Currently selected resolution override. */
  resolution: ResolutionParam;
  /** Invoked with the newly selected resolution. */
  onResolutionChange: (resolution: ResolutionParam) => void;
  /** Categories currently rendered as charts. */
  activeCategories: ChartCategoryKey[];
  /** Invoked with the full set of categories that should be visible. */
  onCategoriesChange: (keys: ChartCategoryKey[]) => void;
}

/** Renders keyboard-operable range, resolution, and category controls for the historical charts view. @param props - Current control values and change handlers. @returns The chart control bar. */
export function ChartControls({
  rangeKey,
  onRangeChange,
  resolution,
  onResolutionChange,
  activeCategories,
  onCategoriesChange,
}: ChartControlsProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-4">
        <ToggleGroup
          aria-label={t("charts.rangeLabel")}
          className="max-w-full flex-wrap bg-muted p-1"
          onValueChange={(value: string) => {
            if (value) onRangeChange(value as RangePresetKey);
          }}
          type="single"
          value={rangeKey}
          size="sm"
        >
          {RANGE_PRESETS.map((preset) => (
            <ToggleGroupItem key={preset.key} value={preset.key}>
              {t(`charts.range.${preset.key}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
        <ToggleGroup
          aria-label={t("charts.resolutionLabel")}
          className="max-w-full flex-wrap bg-muted p-1"
          onValueChange={(value: string) => {
            if (value) onResolutionChange(value as ResolutionParam);
          }}
          type="single"
          value={resolution}
          size="sm"
        >
          {RESOLUTIONS.map((value) => (
            <ToggleGroupItem key={value} value={value}>
              {t(`charts.resolution.${value}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <ToggleGroup
        aria-label={t("charts.categoriesLabel")}
        onValueChange={(value: string[]) => {
          onCategoriesChange(value as ChartCategoryKey[]);
        }}
        spacing={2}
        type="multiple"
        value={activeCategories}
        variant="outline"
      >
        {CATEGORIES.map((category) => (
          <ToggleGroupItem key={category} value={category}>
            {t(`charts.category.${category}`)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}
