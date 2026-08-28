import type { YieldChartPoint } from "@/features/forecasting/utils/yieldMetrics";
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { CartesianGrid, Line, LineChart, XAxis, YAxis } from "recharts";

/** Forward forecast chart properties. */
export interface ForecastYieldChartProps {
  /** Active i18next locale, used for axis formatting. */ locale: string;
  /** Forward forecast points in kilowatts. */ points: YieldChartPoint[];
  /** Disables animation for `prefers-reduced-motion`. */ reducedMotion: boolean;
}

/** Plots the central forward forecast with its lower and upper uncertainty bounds. @param props - Points, locale and motion preference. @returns The forward forecast chart. */
export function ForecastYieldChart({
  locale,
  points,
  reducedMotion,
}: ForecastYieldChartProps) {
  const { t } = useTranslation();
  return (
    <ChartContainer
      className="h-72 w-full"
      config={{
        central: {
          label: t("forecasting.forward.central"),
          color: "var(--chart-1)",
        },
        lower: {
          label: t("forecasting.forward.lower"),
          color: "var(--chart-2)",
        },
        upper: {
          label: t("forecasting.forward.upper"),
          color: "var(--chart-3)",
        },
      }}
    >
      <LineChart accessibilityLayer data={points}>
        <CartesianGrid vertical={false} />
        <XAxis
          dataKey="timestamp"
          tickFormatter={(value: number) =>
            new Intl.DateTimeFormat(locale, {
              weekday: "short",
              hour: "2-digit",
            }).format(value)
          }
        />
        <YAxis tickFormatter={(value: number) => `${String(value)} kW`} />
        <ChartTooltip content={<ChartTooltipContent />} />
        <Line
          dataKey="central"
          dot={false}
          isAnimationActive={!reducedMotion}
          stroke="var(--color-central)"
        />
        <Line
          dataKey="lower"
          dot={false}
          isAnimationActive={!reducedMotion}
          stroke="var(--color-lower)"
          strokeDasharray="4 4"
        />
        <Line
          dataKey="upper"
          dot={false}
          isAnimationActive={!reducedMotion}
          stroke="var(--color-upper)"
          strokeDasharray="4 4"
        />
      </LineChart>
    </ChartContainer>
  );
}
