import type { TooltipValueType } from "recharts";

/** Tooltip value properties. */
export interface ChartTooltipValueProps {
  /** Raw value of the payload item. */ value: TooltipValueType | undefined;
}

/** Renders the numeric value of a tooltip row, localized when numeric. @param props - Raw payload value. @returns The value cell, or nothing when absent. */
export function ChartTooltipValue({ value }: ChartTooltipValueProps) {
  if (value == null) return null;
  return (
    <span className="font-mono font-medium text-foreground tabular-nums">
      {typeof value === "number" ? value.toLocaleString() : String(value)}
    </span>
  );
}
