import { formatTimestamp } from "@/features/charts/utils/formatTimestamp";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/shared/components";
import type { ReactNode } from "react";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

/** A single charted point in its display unit. */
export interface SeriesLineChartPoint {
  /** Timestamp in Unix epoch milliseconds. */ timestamp: number;
  /** Value already converted to its display unit. */ value: number;
}

/** Series line chart properties. */
export interface SeriesLineChartProps {
  /** Points to plot, ordered by timestamp. */ data: SeriesLineChartPoint[];
  /** Localized series label used in the legend/tooltip. */ seriesLabel: string;
  /** Short unit symbol appended to axis ticks, if any. */ unitSymbol: string;
  /** Line color, typically a `--chart-N` design token reference. */ color: string;
  /** Resolution actually returned by the series query. */ actualResolution: string;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Active i18next locale. */ locale: string;
}

/** Renders a resolution-aware time-series line chart with a localized, timezone-aware axis and tooltip. @param props - Chart data, labels, and formatting context. @returns The line chart. */
export function SeriesLineChart({
  data,
  seriesLabel,
  unitSymbol,
  color,
  actualResolution,
  timezone,
  locale,
}: SeriesLineChartProps) {
  const formatTick = (value: number) =>
    formatTimestamp(value, actualResolution, timezone, locale);
  return (
    <ChartContainer
      className="aspect-auto h-64 w-full"
      config={{ value: { label: seriesLabel, color } }}
    >
      <LineChart accessibilityLayer data={data}>
        <CartesianGrid vertical={false} />
        <XAxis dataKey="timestamp" tickFormatter={formatTick} />
        <YAxis
          tickFormatter={(value: number) =>
            unitSymbol ? `${String(value)} ${unitSymbol}` : String(value)
          }
          width={64}
        />
        <ChartTooltip
          content={
            <ChartTooltipContent
              labelFormatter={(label: ReactNode) => formatTick(Number(label))}
            />
          }
        />
        <Line
          dataKey="value"
          dot={false}
          stroke="var(--color-value)"
          type="monotone"
        />
      </LineChart>
    </ChartContainer>
  );
}
