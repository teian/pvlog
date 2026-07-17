/* eslint-disable react/no-multi-comp, max-lines -- Cohesive reporting route family with shared empty, metric, and header renderers. */
import { useSession } from "@/features/auth";
import {
  useSeasonalReport,
  useStatisticsReport,
  useSystemOverviews,
  useWeatherReport,
} from "@/features/reporting";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { AppShell } from "@/widgets";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

function usePageContext() {
  const session = useSession();
  return {
    accountId: session.data?.accountId,
    systemId: session.data?.systemIds[0] ?? "",
    systemIds: session.data?.systemIds ?? [],
  };
}

function Header({
  description,
  title,
}: {
  description: string;
  title: string;
}) {
  return (
    <header className="flex flex-col gap-1">
      <h1 className="text-2xl font-extrabold tracking-tight">{title}</h1>
      <p className="text-sm text-muted-foreground">{description}</p>
    </header>
  );
}

function EmptyOrError({ error, empty }: { error: boolean; empty: boolean }) {
  const { t } = useTranslation();
  if (!error && !empty) return null;
  return (
    <Alert variant={error ? "destructive" : "default"}>
      <AlertTitle>
        {t(error ? "reporting.errorTitle" : "reporting.emptyTitle")}
      </AlertTitle>
      <AlertDescription>
        {t(error ? "reporting.errorDescription" : "reporting.emptyDescription")}
      </AlertDescription>
    </Alert>
  );
}

const energy = (value: number | null, locale: string) =>
  value === null
    ? "—"
    : `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value / 1000)} kWh`;

/** Lists every system visible to the current session. */
export function SystemsPage() {
  const { i18n, t } = useTranslation();
  const context = usePageContext();
  const systems = useSystemOverviews(context.systemIds);
  const failed = systems.some((query) => query.isError);
  const loading = systems.some((query) => query.isPending);
  const rows = systems.flatMap((query) => (query.data ? [query.data] : []));
  return (
    <AppShell systemIds={context.systemIds}>
      <Header
        description={t("reporting.systems.description")}
        title={t("reporting.systems.title")}
      />
      <EmptyOrError
        empty={!loading && context.systemIds.length === 0}
        error={failed}
      />
      {loading ? (
        <p className="text-sm text-muted-foreground">
          {t("reporting.loading")}
        </p>
      ) : null}
      {rows.length > 0 ? (
        <Card>
          <CardHeader>
            <CardTitle>{t("reporting.systems.cardTitle")}</CardTitle>
          </CardHeader>
          <CardContent>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>{t("reporting.systems.name")}</TableHead>
                  <TableHead>{t("reporting.systems.capacity")}</TableHead>
                  <TableHead>{t("reporting.systems.equipment")}</TableHead>
                  <TableHead>{t("reporting.systems.status")}</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {rows.map((system) => (
                  <TableRow key={system.id}>
                    <TableCell>
                      <Link
                        className="font-semibold text-primary hover:underline"
                        to={`/systems/${system.id}`}
                      >
                        {system.name}
                      </Link>
                      <div className="text-xs text-muted-foreground">
                        {system.timezone}
                      </div>
                    </TableCell>
                    <TableCell>
                      {system.capacityWatts === null
                        ? "—"
                        : `${new Intl.NumberFormat(i18n.language).format(system.capacityWatts / 1000)} kWp`}
                    </TableCell>
                    <TableCell>
                      {t("reporting.systems.equipmentValue", {
                        inverters: system.inverterCount,
                        strings: system.stringCount,
                      })}
                    </TableCell>
                    <TableCell>
                      <span className="inline-flex items-center gap-2 font-medium uppercase text-success">
                        <span className="size-2 rounded-full bg-success" />
                        {t(`reporting.lifecycle.${system.lifecycle}`, {
                          defaultValue: system.lifecycle,
                        })}
                      </span>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      ) : null}
    </AppShell>
  );
}

/** Displays lifetime totals and month-by-month production. */
export function StatisticsPage() {
  const { i18n, t } = useTranslation();
  const context = usePageContext();
  const report = useStatisticsReport(context.systemId);
  const values = report.data;
  return (
    <AppShell systemIds={context.systemIds}>
      <Header
        description={t("reporting.statistics.description")}
        title={t("reporting.statistics.title")}
      />
      <EmptyOrError empty={!context.systemId} error={report.isError} />
      {report.isPending && context.systemId ? (
        <p className="text-sm text-muted-foreground">
          {t("reporting.loading")}
        </p>
      ) : null}
      {values ? (
        <>
          <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
            <Metric
              label={t("reporting.statistics.totalGeneration")}
              value={energy(values.generationEnergyWh, i18n.language)}
            />
            <Metric
              label={t("reporting.statistics.totalConsumption")}
              value={energy(values.consumptionEnergyWh, i18n.language)}
            />
            <Metric
              label={t("reporting.statistics.peak")}
              value={
                values.peakGenerationPowerWatts === null
                  ? "—"
                  : `${new Intl.NumberFormat(i18n.language).format(values.peakGenerationPowerWatts / 1000)} kW`
              }
            />
            <Metric
              label={t("reporting.statistics.coverage")}
              value={`${(values.coverageBasisPoints / 100).toFixed(1)}%`}
            />
          </div>
          <Card>
            <CardHeader>
              <CardTitle>{t("reporting.statistics.monthly")}</CardTitle>
              <CardDescription>
                {t("reporting.statistics.monthlyDescription")}
              </CardDescription>
            </CardHeader>
            <CardContent>
              {values.monthly.length === 0 ? (
                <p className="text-sm text-muted-foreground">
                  {t("reporting.noMeasurements")}
                </p>
              ) : (
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead>{t("reporting.period")}</TableHead>
                      <TableHead>{t("reporting.generation")}</TableHead>
                      <TableHead>{t("reporting.consumption")}</TableHead>
                      <TableHead>{t("reporting.coverage")}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {values.monthly.map((month) => (
                      <TableRow key={month.bucketStartEpochMillis}>
                        <TableCell>
                          {new Intl.DateTimeFormat(i18n.language, {
                            month: "long",
                            year: "numeric",
                          }).format(new Date(month.bucketStartEpochMillis))}
                        </TableCell>
                        <TableCell className="font-semibold text-primary">
                          {energy(month.generationEnergyWh, i18n.language)}
                        </TableCell>
                        <TableCell>
                          {energy(month.consumptionEnergyWh, i18n.language)}
                        </TableCell>
                        <TableCell>
                          {(month.coverageBasisPoints / 100).toFixed(1)}
                          {"%"}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              )}
            </CardContent>
          </Card>
        </>
      ) : null}
    </AppShell>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <Card>
      <CardHeader>
        <CardDescription className="font-semibold uppercase tracking-wide">
          {label}
        </CardDescription>
        <CardTitle className="font-mono text-2xl text-primary">
          {value}
        </CardTitle>
      </CardHeader>
    </Card>
  );
}

/** Aggregates daily production by meteorological season. */
export function SeasonalPage() {
  const { i18n, t } = useTranslation();
  const context = usePageContext();
  const report = useSeasonalReport(context.systemId);
  const maximum = Math.max(
    1,
    ...(report.data?.seasons.map((season) => season.generationEnergyWh) ?? []),
  );
  return (
    <AppShell systemIds={context.systemIds}>
      <Header
        description={t("reporting.seasonal.description")}
        title={t("reporting.seasonal.title")}
      />
      <EmptyOrError empty={!context.systemId} error={report.isError} />
      {report.data ? (
        <Card>
          <CardHeader>
            <CardTitle>{t("reporting.seasonal.cardTitle")}</CardTitle>
            <CardDescription>
              {t("reporting.seasonal.cardDescription")}
            </CardDescription>
          </CardHeader>
          <CardContent className="grid gap-5 sm:grid-cols-2">
            {report.data.seasons.map((season) => (
              <div className="space-y-2" key={season.season}>
                <div className="flex justify-between gap-4">
                  <span className="font-semibold">
                    {t(`reporting.seasonal.seasons.${season.season}`)}
                  </span>
                  <span className="font-mono text-primary">
                    {energy(season.generationEnergyWh, i18n.language)}
                  </span>
                </div>
                <div className="h-2 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full bg-primary"
                    style={{
                      width: `${String((season.generationEnergyWh / maximum) * 100)}%`,
                    }}
                  />
                </div>
                <p className="text-xs text-muted-foreground">
                  {t("reporting.seasonal.days", {
                    count: season.measuredDays,
                    average: energy(season.averageDailyEnergyWh, i18n.language),
                  })}
                </p>
              </div>
            ))}
          </CardContent>
        </Card>
      ) : null}
    </AppShell>
  );
}

/** Shows the latest provider weather inputs and derived energy forecast. */
export function WeatherPage() {
  const { i18n, t } = useTranslation();
  const context = usePageContext();
  const report = useWeatherReport(context.systemId);
  return (
    <AppShell systemIds={context.systemIds}>
      <Header
        description={t("reporting.weather.description")}
        title={t("reporting.weather.title")}
      />
      <EmptyOrError empty={!context.systemId} error={report.isError} />
      {report.data ? (
        <Card>
          <CardHeader>
            <CardTitle>{t("reporting.weather.cardTitle")}</CardTitle>
            <CardDescription>
              {report.data.attribution ?? t("reporting.weather.noAttribution")}
            </CardDescription>
          </CardHeader>
          <CardContent>
            {report.data.points.length === 0 ? (
              <p className="text-sm text-muted-foreground">
                {t("reporting.weather.noForecast")}
              </p>
            ) : (
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>{t("reporting.period")}</TableHead>
                    <TableHead>{t("reporting.weather.irradiance")}</TableHead>
                    <TableHead>{t("reporting.weather.temperature")}</TableHead>
                    <TableHead>{t("reporting.weather.clouds")}</TableHead>
                    <TableHead>{t("reporting.weather.predicted")}</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {report.data.points.map((point) => (
                    <TableRow key={point.intervalStartEpochMillis}>
                      <TableCell>
                        {new Intl.DateTimeFormat(i18n.language, {
                          dateStyle: "medium",
                          timeStyle: "short",
                        }).format(new Date(point.intervalStartEpochMillis))}
                      </TableCell>
                      <TableCell>
                        {point.irradianceWattsPerSquareMetre === null
                          ? "—"
                          : `${String(point.irradianceWattsPerSquareMetre)} W/m²`}
                      </TableCell>
                      <TableCell>
                        {point.ambientTemperatureMillicelsius === null
                          ? "—"
                          : `${(point.ambientTemperatureMillicelsius / 1000).toFixed(1)} °C`}
                      </TableCell>
                      <TableCell>
                        {point.cloudCoverBasisPoints === null
                          ? "—"
                          : `${(point.cloudCoverBasisPoints / 100).toFixed(0)}%`}
                      </TableCell>
                      <TableCell className="font-semibold text-primary">
                        {energy(point.predictedEnergyWh, i18n.language)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            )}
          </CardContent>
        </Card>
      ) : null}
    </AppShell>
  );
}
