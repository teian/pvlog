/** Predefined historical range lengths, shared across chart and data-quality views. */
export type RangePresetKey =
  "day" | "week" | "month" | "year" | "fiveYears" | "all";

/** A selectable historical range preset. */
export interface RangePreset {
  /** Stable preset identifier. */ key: RangePresetKey;
  /** Preset duration in milliseconds, ending at the current time. */
  durationMillis: number;
}

const DAY_MILLIS = 86_400_000;

/** Range presets offered by range controls, from one day to 25 years. */
export const RANGE_PRESETS: [RangePreset, ...RangePreset[]] = [
  { key: "day", durationMillis: DAY_MILLIS },
  { key: "week", durationMillis: DAY_MILLIS * 7 },
  { key: "month", durationMillis: DAY_MILLIS * 30 },
  { key: "year", durationMillis: DAY_MILLIS * 365 },
  { key: "fiveYears", durationMillis: DAY_MILLIS * 365 * 5 },
  { key: "all", durationMillis: DAY_MILLIS * 365 * 25 },
];
