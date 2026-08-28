import { ChartTooltipItem } from "@/shared/components/Chart/ChartTooltipItem";
import { ChartTooltipLabel } from "@/shared/components/Chart/ChartTooltipLabel";
import { useChart } from "@/shared/components/Chart/chartConfig";
import type { ChartTooltipContentProps } from "@/shared/components/Chart/chartTooltip.types";
import { cn } from "@/shared/lib/utils";

/** Renders the chart tooltip: a resolved label plus one row per visible series. @param props - Recharts tooltip state and presentation options. @returns The tooltip surface, or nothing while inactive. */
export function ChartTooltipContent({
  active,
  payload,
  className,
  indicator = "dot",
  hideLabel = false,
  hideIndicator = false,
  label,
  labelFormatter,
  labelClassName,
  formatter,
  color,
  nameKey,
  labelKey,
}: ChartTooltipContentProps) {
  const { config } = useChart();

  if (!active || !payload?.length) return null;

  const nestLabel = payload.length === 1 && indicator !== "dot";
  const tooltipLabel = (
    <ChartTooltipLabel
      hideLabel={hideLabel}
      label={label}
      labelClassName={labelClassName}
      labelFormatter={labelFormatter}
      labelKey={labelKey}
      payload={payload}
    />
  );

  return (
    <div
      className={cn(
        "grid min-w-[8rem] items-start gap-1.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl",
        className,
      )}
    >
      {nestLabel ? null : tooltipLabel}
      <div className="grid gap-1.5">
        {payload
          .filter((item) => item.type !== "none")
          .map((item, index) => (
            <ChartTooltipItem
              color={color}
              config={config}
              formatter={formatter}
              hideIndicator={hideIndicator}
              index={index}
              indicator={indicator}
              item={item}
              key={index}
              nameKey={nameKey}
              nestLabel={nestLabel}
              payload={payload}
              tooltipLabel={tooltipLabel}
            />
          ))}
      </div>
    </div>
  );
}
