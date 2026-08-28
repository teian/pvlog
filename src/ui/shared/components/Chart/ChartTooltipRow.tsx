import { ChartTooltipValue } from "@/shared/components/Chart/ChartTooltipValue";
import { cn } from "@/shared/lib/utils";
import type * as React from "react";
import type { TooltipValueType } from "recharts";

/** Tooltip row properties. */
export interface ChartTooltipRowProps {
  /** Label from the chart config, if the series defines one. */ configLabel: React.ReactNode;
  /** Series name, used when the config carries no label. */ name: React.ReactNode;
  /** Whether the tooltip label is nested inside this row. */ nestLabel: boolean;
  /** Tooltip label rendered when nested. */ nestedLabel: React.ReactNode;
  /** Raw value of the payload item. */ value: TooltipValueType | undefined;
}

/** Renders the label and value of one tooltip row. @param props - Labels, nesting mode and value. @returns The tooltip row. */
export function ChartTooltipRow({
  configLabel,
  name,
  nestLabel,
  nestedLabel,
  value,
}: ChartTooltipRowProps) {
  return (
    <div
      className={cn(
        "flex flex-1 justify-between leading-none",
        nestLabel ? "items-end" : "items-center",
      )}
    >
      <div className="grid gap-1.5">
        {nestLabel ? nestedLabel : null}
        <span className="text-muted-foreground">{configLabel ?? name}</span>
      </div>
      <ChartTooltipValue value={value} />
    </div>
  );
}
