import type { PerformanceChartPoint } from "@/features/forecasting/utils/performanceMetrics";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

/** Performance line chart properties. */
export interface PerformanceLineChartProps {
  /** Active i18next locale, used for axis formatting. */ locale: string;
  /** Actual-versus-modeled points in kilowatt-hours. */ points: PerformanceChartPoint[];
  /** Disables animation for `prefers-reduced-motion`. */ reducedMotion: boolean;
}

/** Plots measured against modeled energy over the selected range. @param props - Points, locale and motion preference. @returns The comparison line chart. */
export function PerformanceLineChart({
  locale,
  points,
  reducedMotion,
}: PerformanceLineChartProps) {
  const { t } = useTranslation();
  return (
    <ChartContainer
      className="h-64 w-full"
      config={{
        actual: {
          label: t("forecasting.performance.actual"),
          color: "var(--chart-1)",
        },
        modeled: {
          label: t("forecasting.performance.modeled"),
          color: "var(--chart-3)",
        },
      }}
    >
      <LineChart accessibilityLayer data={points}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="timestamp"
          tickFormatter={(value: number) =>
            new Intl.DateTimeFormat(locale, { dateStyle: "short" }).format(
              value,
            )
          }
        />
        <YAxis tickFormatter={(value: number) => `${String(value)} kWh`} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Line
          dataKey="actual"
          dot={false}
          isAnimationActive={!reducedMotion}
          stroke="var(--color-actual)"
        />
        <Line
          dataKey="modeled"
          dot={false}
          isAnimationActive={!reducedMotion}
          stroke="var(--color-modeled)"
          strokeDasharray="4 4"
        />
      </LineChart>
    </ChartContainer>
  );
}
