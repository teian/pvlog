import * as React from "react";

/** Theme name to CSS selector mapping used when emitting chart color variables. */
export const THEMES = { light: "", dark: ".dark" } as const;

/** Per-series presentation config keyed by the series `dataKey`. */
export type ChartConfig = Record<
  string,
  {
    label?: React.ReactNode;
    icon?: React.ComponentType;
  } & (
    | { color?: string; theme?: never }
    | { color?: never; theme: Record<keyof typeof THEMES, string> }
  )
>;

interface ChartContextProps {
  config: ChartConfig;
}

const ChartContext = React.createContext<ChartContextProps | null>(null);

/** Provides the chart config to descendants of a `ChartContainer`. */
export const ChartContextProvider = ChartContext.Provider;

/** Reads the chart config of the enclosing `ChartContainer`. @returns The active chart config. @throws Error - When used outside a `ChartContainer`. */
export function useChart(): ChartContextProps {
  const context = React.useContext(ChartContext);

  if (!context) {
    throw new Error("useChart must be used within a <ChartContainer />");
  }

  return context;
}

/** Returns the nested `payload.payload` object when the item carries one. */
function nestedPayload(payload: object): object | undefined {
  if (!("payload" in payload)) return undefined;
  const nested = payload.payload;
  return typeof nested === "object" && nested !== null ? nested : undefined;
}

/** Reads `key` from an object when it holds a string. */
function stringProperty(source: object, key: string): string | undefined {
  if (!(key in source)) return undefined;
  const value = source[key as keyof typeof source];
  return typeof value === "string" ? value : undefined;
}

/** Resolves the config entry describing one payload item, preferring a label key carried by the item itself. @param config - Active chart config. @param payload - Recharts payload item. @param key - Fallback config key. @returns The matching config entry, if any. */
export function getPayloadConfigFromPayload(
  config: ChartConfig,
  payload: unknown,
  key: string,
): ChartConfig[string] | undefined {
  if (typeof payload !== "object" || payload === null) return undefined;

  const nested = nestedPayload(payload);
  const configLabelKey =
    stringProperty(payload, key) ??
    (nested === undefined ? undefined : stringProperty(nested, key)) ??
    key;

  return configLabelKey in config ? config[configLabelKey] : config[key];
}

/** Picks the config key of a payload item, honouring an explicit `nameKey` override. @param item - Recharts payload item. @param nameKey - Explicit config key override. @param fallbacks - Item fields to try, in order. @returns The resolved key. */
export function resolveItemKey(
  nameKey: string | undefined,
  fallbacks: readonly unknown[],
): string {
  const rawKey = nameKey ?? fallbacks.find((value) => value != null);
  if (typeof rawKey === "string") return rawKey;
  if (typeof rawKey === "number") return String(rawKey);
  return "value";
}

/** Resolves the indicator color of a tooltip item, preferring an explicit color, then the item fill, then its series color. @param explicit - Color passed to the tooltip. @param item - Recharts payload item. @returns The color to render, if any. */
export function resolveIndicatorColor(
  explicit: string | undefined,
  item: { color?: unknown; payload?: unknown },
): string | undefined {
  if (explicit !== undefined) return explicit;
  const fill = (item.payload as { fill?: unknown } | undefined)?.fill;
  if (typeof fill === "string") return fill;
  return typeof item.color === "string" ? item.color : undefined;
}
