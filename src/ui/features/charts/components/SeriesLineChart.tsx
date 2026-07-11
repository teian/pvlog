import type { SeriesGap } from "@/features/charts/api/chartsApi";
import { formatTimestamp } from "@/features/charts/utils/formatTimestamp";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/shared/components";
import { type ReactNode, useId } from "react";
import {
  Brush,
  CartesianGrid,
  Line,
  LineChart,
  ReferenceArea,
  XAxis,
  YAxis,
} from "recharts";

/** A single charted point in its display unit. */
export interface SeriesLineChartPoint {
  /** Timestamp in Unix epoch milliseconds. */ timestamp: number;
  /** Value already converted to its display unit. */ value: number;
}

/** Series line chart properties. */
export interface SeriesLineChartProps {
  /** Points to plot, ordered by timestamp. */ data: SeriesLineChartPoint[];
  /** Missing/suspect/incomplete-coverage intervals shaded on the chart. */
  gaps: SeriesGap[];
  /** Localized series label used in the legend/tooltip. */ seriesLabel: string;
  /** Short unit symbol appended to axis ticks, if any. */ unitSymbol: string;
  /** Line color, typically a `--chart-N` design token reference. */ color: string;
  /** Resolution actually returned by the series query. */ actualResolution: string;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Active i18next locale. */ locale: string;
  /** Disables chart animation for `prefers-reduced-motion`. */
  reducedMotion: boolean;
}

/** Renders a resolution-aware, zoomable time-series line chart with a localized, timezone-aware axis, tooltip, and shaded gap intervals. @param props - Chart data, gaps, labels, and formatting context. @returns The line chart. */
export function SeriesLineChart({
  data,
  gaps,
  seriesLabel,
  unitSymbol,
  color,
  actualResolution,
  timezone,
  locale,
  reducedMotion,
}: SeriesLineChartProps) {
  const hatchId = useId();
  const formatTick = (value: number) =>
    formatTimestamp(value, actualResolution, timezone, locale);
  return (
    <ChartContainer
      className="aspect-auto h-64 w-full"
      config={{ value: { label: seriesLabel, color } }}
    >
      <LineChart accessibilityLayer data={data}>
        <defs>
          <pattern
            height="8"
            id={hatchId}
            patternTransform="rotate(45)"
            patternUnits="userSpaceOnUse"
            width="8"
          >
            <rect className="fill-muted" height="8" width="8" />
            <line
              className="stroke-muted-foreground"
              strokeWidth="2"
              x1="0"
              x2="0"
              y1="0"
              y2="8"
            />
          </pattern>
        </defs>
        <CartesianGrid vertical={false} />
        {gaps.map((gap) => (
          <ReferenceArea
            fill={`url(#${hatchId})`}
            fillOpacity={1}
            key={`${String(gap.startEpochMillis)}-${gap.kind}`}
            stroke="none"
            x1={gap.startEpochMillis}
            x2={gap.endEpochMillis}
          />
        ))}
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
          isAnimationActive={!reducedMotion}
          stroke="var(--color-value)"
          type="monotone"
        />
        {data.length > 1 ? (
          <Brush
            dataKey="timestamp"
            height={24}
            stroke="var(--color-value)"
            tickFormatter={formatTick}
            travellerWidth={10}
          />
        ) : null}
      </LineChart>
    </ChartContainer>
  );
}
