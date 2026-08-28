import {
  type ChartConfig,
  getPayloadConfigFromPayload,
  resolveItemKey,
  useChart,
} from "@/shared/components/Chart/chartConfig";
import type {
  ChartTooltipContentProps,
  TooltipPayload,
} from "@/shared/components/Chart/chartTooltip.types";
import { cn } from "@/shared/lib/utils";
import type * as React from "react";

/** Picks the label to display, preferring an explicit config entry for a string label. */
function configuredLabel(
  config: ChartConfig,
  label: ChartTooltipContentProps["label"],
  labelKey: string | undefined,
  itemConfig: ChartConfig[string] | undefined,
): React.ReactNode {
  if (labelKey === undefined && typeof label === "string") {
    return config[label]?.label ?? label;
  }
  return itemConfig?.label;
}

/** Tooltip label properties. */
export interface ChartTooltipLabelProps {
  /** Suppresses the label entirely. */ hideLabel: boolean;
  /** Raw label supplied by recharts. */ label: ChartTooltipContentProps["label"];
  /** Extra classes for the label element. */ labelClassName:
    string | undefined;
  /** Caller-supplied label renderer. */ labelFormatter: ChartTooltipContentProps["labelFormatter"];
  /** Explicit config key override for the label. */ labelKey:
    string | undefined;
  /** Payload the label describes. */ payload: TooltipPayload;
}

/** Renders the heading of a chart tooltip, resolved from the chart config or a caller-supplied formatter. @param props - Label sources and presentation flags. @returns The tooltip label, or nothing when suppressed. */
export function ChartTooltipLabel({
  hideLabel,
  label,
  labelClassName,
  labelFormatter,
  labelKey,
  payload,
}: ChartTooltipLabelProps) {
  const { config } = useChart();
  if (hideLabel || payload.length === 0) return null;

  const item = payload[0];
  const key = resolveItemKey(labelKey, [item?.dataKey, item?.name]);
  const itemConfig = getPayloadConfigFromPayload(config, item, key);
  const value = configuredLabel(config, label, labelKey, itemConfig);

  if (labelFormatter)
    return (
      <div className={cn("font-medium", labelClassName)}>
        {labelFormatter(value, payload)}
      </div>
    );
  if (!value) return null;
  return <div className={cn("font-medium", labelClassName)}>{value}</div>;
}
