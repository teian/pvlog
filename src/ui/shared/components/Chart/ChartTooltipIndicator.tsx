import type { TooltipIndicator } from "@/shared/components/Chart/chartTooltip.types";
import { cn } from "@/shared/lib/utils";
import type * as React from "react";

/** Tooltip indicator properties. */
export interface ChartTooltipIndicatorProps {
  /** Resolved marker color. */ color: string | undefined;
  /** Marker style. */ indicator: TooltipIndicator;
  /** Whether the tooltip label is nested beside the marker. */ nestLabel: boolean;
}

/** Renders the per-series color marker of a tooltip row. @param props - Color, marker style and nesting mode. @returns The indicator marker. */
export function ChartTooltipIndicator({
  color,
  indicator,
  nestLabel,
}: ChartTooltipIndicatorProps) {
  return (
    <div
      className={cn(
        "shrink-0 rounded-[2px] border-(--color-border) bg-(--color-bg)",
        {
          "h-2.5 w-2.5": indicator === "dot",
          "w-1": indicator === "line",
          "w-0 border-[1.5px] border-dashed bg-transparent":
            indicator === "dashed",
          "my-0.5": nestLabel && indicator === "dashed",
        },
      )}
      style={
        {
          "--color-bg": color,
          "--color-border": color,
        } as React.CSSProperties
      }
    />
  );
}
