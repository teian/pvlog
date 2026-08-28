import { ChartTooltipIndicator } from "@/shared/components/Chart/ChartTooltipIndicator";
import { ChartTooltipRow } from "@/shared/components/Chart/ChartTooltipRow";
import {
  type ChartConfig,
  getPayloadConfigFromPayload,
  resolveIndicatorColor,
  resolveItemKey,
} from "@/shared/components/Chart/chartConfig";
import type {
  TooltipFormatter,
  TooltipIndicator,
  TooltipPayload,
  TooltipPayloadItem,
} from "@/shared/components/Chart/chartTooltip.types";
import { cn } from "@/shared/lib/utils";
import type * as React from "react";

/** Tooltip item properties. */
export interface ChartTooltipItemProps {
  /** Explicit indicator color overriding the series color. */ color:
    string | undefined;
  /** Active chart config. */ config: ChartConfig;
  /** Optional caller-supplied renderer for the whole item. */ formatter: TooltipFormatter;
  /** Hides the color marker. */ hideIndicator: boolean;
  /** Position of the item within the payload. */ index: number;
  /** Marker style. */ indicator: TooltipIndicator;
  /** Payload entry being rendered. */ item: TooltipPayloadItem;
  /** Explicit config key override. */ nameKey: string | undefined;
  /** Whether the tooltip label is nested inside the row. */ nestLabel: boolean;
  /** Full payload, passed through to the formatter. */ payload: TooltipPayload;
  /** Tooltip label rendered when nested. */ tooltipLabel: React.ReactNode;
}

/** Renders one series entry of the chart tooltip, delegating to a caller-supplied formatter when one applies. @param props - Payload entry and presentation context. @returns The tooltip item. */
export function ChartTooltipItem({
  color,
  config,
  formatter,
  hideIndicator,
  index,
  indicator,
  item,
  nameKey,
  nestLabel,
  payload,
  tooltipLabel,
}: ChartTooltipItemProps) {
  const key = resolveItemKey(nameKey, [item.name, item.dataKey]);
  const itemConfig = getPayloadConfigFromPayload(config, item, key);
  const Icon = itemConfig?.icon;

  return (
    <div
      className={cn(
        "flex w-full flex-wrap items-stretch gap-2 [&>svg]:h-2.5 [&>svg]:w-2.5 [&>svg]:text-muted-foreground",
        indicator === "dot" && "items-center",
      )}
    >
      {formatter && item.value !== undefined && item.name ? (
        formatter(item.value, item.name, item, index, payload)
      ) : (
        <>
          {Icon ? <Icon /> : null}
          {!Icon && !hideIndicator ? (
            <ChartTooltipIndicator
              color={resolveIndicatorColor(color, item)}
              indicator={indicator}
              nestLabel={nestLabel}
            />
          ) : null}
          <ChartTooltipRow
            configLabel={itemConfig?.label}
            name={item.name}
            nestLabel={nestLabel}
            nestedLabel={tooltipLabel}
            value={item.value}
          />
        </>
      )}
    </div>
  );
}
