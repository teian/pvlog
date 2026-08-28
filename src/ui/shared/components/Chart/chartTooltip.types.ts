import type * as React from "react";
import type * as RechartsPrimitive from "recharts";
import type { TooltipValueType } from "recharts";

/** Series name type carried by recharts tooltip payloads. */
export type TooltipNameType = number | string;

/** Full prop surface of the chart tooltip content. */
export type ChartTooltipContentProps = React.ComponentProps<
  typeof RechartsPrimitive.Tooltip
> &
  React.ComponentProps<"div"> & {
    hideLabel?: boolean;
    hideIndicator?: boolean;
    indicator?: TooltipIndicator;
    nameKey?: string;
    labelKey?: string;
  } & Omit<
    RechartsPrimitive.DefaultTooltipContentProps<
      TooltipValueType,
      TooltipNameType
    >,
    "accessibilityLayer"
  >;

/** Visual style of the per-series marker in the tooltip. */
export type TooltipIndicator = "dashed" | "dot" | "line";

/** Payload list handed to the tooltip content. */
export type TooltipPayload = NonNullable<ChartTooltipContentProps["payload"]>;

/** One entry of a tooltip payload. */
export type TooltipPayloadItem = TooltipPayload[number];

/** Optional per-item formatter supplied by the caller. */
export type TooltipFormatter = ChartTooltipContentProps["formatter"];
