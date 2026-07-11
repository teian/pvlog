import type { SeriesField } from "@/features/charts/api/chartsApi";

/** Chart category grouping one or more related series fields. */
export type ChartCategoryKey =
  | "generation"
  | "consumption"
  | "grid"
  | "battery"
  | "environment"
  | "extended";

/** A chart category and the series fields it can display. */
export interface ChartCategoryDefinition {
  /** Stable category identifier. */ key: ChartCategoryKey;
  /** Selectable fields, most commonly requested first. */
  fields: [SeriesField, ...SeriesField[]];
}

/**
 * Categories available on the historical charts page. Financial values are
 * exposed only through period statistics, not the bounded series endpoint,
 * so they are not selectable here.
 */
export const CHART_CATEGORIES: ChartCategoryDefinition[] = [
  { key: "generation", fields: ["generation_power", "generation_energy"] },
  { key: "consumption", fields: ["consumption_power", "consumption_energy"] },
  { key: "grid", fields: ["grid_power"] },
  { key: "battery", fields: ["battery_power", "battery_state_of_charge"] },
  { key: "environment", fields: ["temperature"] },
  { key: "extended", fields: ["extended"] },
];
