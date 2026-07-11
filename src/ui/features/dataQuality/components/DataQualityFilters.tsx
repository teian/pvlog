import type { DataQualityKind } from "@/features/dataQuality/api/dataQualityApi";
import { ToggleGroup, ToggleGroupItem } from "@/shared/components";
import { RANGE_PRESETS, type RangePresetKey } from "@/shared/lib";
import { useTranslation } from "react-i18next";

const KINDS: DataQualityKind[] = [
  "missing_interval",
  "suspect_observation",
  "source_conflict",
  "counter_reset",
  "rejected_ingestion",
  "aggregate_lag",
];

/** Data quality filter properties. */
export interface DataQualityFiltersProps {
  /** Currently selected range preset. */ rangeKey: RangePresetKey;
  /** Invoked with the newly selected range preset. */
  onRangeChange: (key: RangePresetKey) => void;
  /** Issue kinds currently shown. */ activeKinds: DataQualityKind[];
  /** Invoked with the full set of kinds that should be visible. */
  onKindsChange: (kinds: DataQualityKind[]) => void;
}

/** Renders range and issue-kind filter controls for the data-quality inspection view. @param props - Current filter values and change handlers. @returns The data-quality filter bar. */
export function DataQualityFilters({
  rangeKey,
  onRangeChange,
  activeKinds,
  onKindsChange,
}: DataQualityFiltersProps) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-3">
      <ToggleGroup
        aria-label={t("dataQuality.rangeLabel")}
        onValueChange={(value: string) => {
          if (value) onRangeChange(value as RangePresetKey);
        }}
        type="single"
        value={rangeKey}
        variant="outline"
      >
        {RANGE_PRESETS.map((preset) => (
          <ToggleGroupItem key={preset.key} value={preset.key}>
            {t(`charts.range.${preset.key}`)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
      <ToggleGroup
        aria-label={t("dataQuality.kindsLabel")}
        onValueChange={(value: string[]) => {
          onKindsChange(value as DataQualityKind[]);
        }}
        type="multiple"
        value={activeKinds}
        variant="outline"
      >
        {KINDS.map((kind) => (
          <ToggleGroupItem key={kind} value={kind}>
            {t(`dataQuality.kind.${kind}`)}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </div>
  );
}
