import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { cn } from "@/shared/lib/utils";

/** Metric displayed in the aggregate dashboard grid. */
export interface DashboardMetric {
  /** Stable metric identity. */
  key: string;
  /** Localized metric label. */
  label: string;
  /** Formatted metric value. */
  value: string;
  /** Semantic value emphasis class. */
  valueClassName: string;
}

/** Dashboard metric grid properties. */
export interface DashboardMetricGridProps {
  /** Metrics displayed in reading order. */
  metrics: DashboardMetric[];
}

/** Renders aggregate live metrics in compact cards. @param props - Formatted dashboard metrics. @returns The responsive metric grid. */
export function DashboardMetricGrid({ metrics }: DashboardMetricGridProps) {
  return (
    <div className="grid grid-cols-2 gap-4 min-[900px]:grid-cols-4">
      {metrics.map((metric) => (
        <Card key={metric.key}>
          <CardHeader className="gap-3">
            <CardDescription className="text-[10px] font-semibold uppercase tracking-widest">
              {metric.label}
            </CardDescription>
            <CardTitle
              className={cn(
                "font-mono text-[1.7rem] font-bold tracking-tight tabular-nums",
                metric.valueClassName,
              )}
            >
              {metric.value}
            </CardTitle>
          </CardHeader>
        </Card>
      ))}
    </div>
  );
}
