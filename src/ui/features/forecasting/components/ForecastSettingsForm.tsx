import type { VersionedForecastSettings } from "@/features/forecasting/api/forecastApi";
import { ForecastSettingsIdentityFields } from "@/features/forecasting/components/ForecastSettingsIdentityFields";
import { ForecastSettingsStatus } from "@/features/forecasting/components/ForecastSettingsStatus";
import { useUpdateForecastSettings } from "@/features/forecasting/hooks/useForecasting";
import { forecastSettingsInputSchema } from "@/features/forecasting/types/forecast.types";
import {
  FORECAST_LOSS_FIELDS,
  forecastSettingsDefaults,
  forecastSettingsFormSchema,
  type ForecastSettingsFormValues,
} from "@/features/forecasting/utils/forecastSettingsForm";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Field,
  FieldError,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useEffect } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";

/** Settings editor properties. */
export interface ForecastSettingsFormProps {
  /** Owning account. */ accountId: string;
  /** Target system. */ systemId: string;
  /** Loaded settings and ETag. */ versioned?: VersionedForecastSettings;
  /** Whether settings are loading. */ loading: boolean;
  /** Whether settings failed to load. */ error: boolean;
}

/** Edits effective-dated forecast losses and calibration with optimistic concurrency. @param props - Scope and loaded settings. @returns Validated settings form. */
export function ForecastSettingsForm({
  accountId,
  systemId,
  versioned,
  loading,
  error,
}: ForecastSettingsFormProps) {
  const { t } = useTranslation();
  const update = useUpdateForecastSettings(accountId, systemId);
  const form = useForm<ForecastSettingsFormValues>({
    resolver: zodResolver(forecastSettingsFormSchema),
    defaultValues: forecastSettingsDefaults(versioned),
  });
  useEffect(() => {
    form.reset(forecastSettingsDefaults(versioned));
  }, [form, versioned]);
  if (loading) return <Card className="h-80 animate-pulse bg-muted" />;
  if (error || !versioned)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("forecasting.settings.errorTitle")}</AlertTitle>
        <AlertDescription>
          {t("forecasting.settings.errorDescription")}
        </AlertDescription>
      </Alert>
    );
  const submit = form.handleSubmit(async (values) => {
    const parsed = forecastSettingsFormSchema.parse(values);
    const input = forecastSettingsInputSchema.parse({
      effectiveFrom: parsed.effectiveFrom,
      effectiveTo: parsed.effectiveTo === "" ? null : parsed.effectiveTo,
      modelIdentifier: parsed.modelIdentifier,
      modelRevision: parsed.modelRevision,
      calibrationBasisPoints: parsed.calibrationBasisPoints,
      losses: {
        soilingBasisPoints: parsed.soilingBasisPoints,
        shadingBasisPoints: parsed.shadingBasisPoints,
        mismatchBasisPoints: parsed.mismatchBasisPoints,
        wiringBasisPoints: parsed.wiringBasisPoints,
        unavailabilityBasisPoints: parsed.unavailabilityBasisPoints,
      },
    });
    await update.mutateAsync({ input, etag: versioned.etag });
  });
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("forecasting.settings.title")}</CardTitle>
        <CardDescription>
          {t("forecasting.settings.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form
          className="grid gap-4 md:grid-cols-2"
          onSubmit={(event) => {
            void submit(event);
          }}
        >
          <ForecastSettingsIdentityFields register={form.register} />
          {FORECAST_LOSS_FIELDS.map((name) => (
            <Field key={name}>
              <FieldLabel htmlFor={`forecast-${name}`}>
                {t(`forecasting.settings.${name}`)}
              </FieldLabel>
              <Input
                aria-invalid={Boolean(form.formState.errors[name])}
                id={`forecast-${name}`}
                type="number"
                {...form.register(name)}
              />
              <FieldError errors={[form.formState.errors[name]]} />
            </Field>
          ))}
          <div className="flex items-center gap-3 md:col-span-2">
            <Button disabled={update.isPending} type="submit">
              {update.isPending
                ? t("forecasting.settings.saving")
                : t("forecasting.settings.save")}
            </Button>
            <ForecastSettingsStatus
              error={update.isError}
              success={update.isSuccess}
            />
          </div>
        </form>
      </CardContent>
    </Card>
  );
}
