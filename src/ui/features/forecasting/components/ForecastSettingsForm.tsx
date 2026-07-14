import type { VersionedForecastSettings } from "@/features/forecasting/api/forecastApi";
import { useUpdateForecastSettings } from "@/features/forecasting/hooks/useForecasting";
import { forecastSettingsInputSchema } from "@/features/forecasting/types/forecast.types";
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
import { z } from "zod";

const formSchema = z.object({
  effectiveFrom: z.coerce.number().int(),
  effectiveTo: z.union([z.literal(""), z.coerce.number().int()]),
  modelIdentifier: z.string().min(1).max(64),
  modelRevision: z.coerce.number().int().min(1).max(65_535),
  soilingBasisPoints: z.coerce.number().int().min(0).max(10_000),
  shadingBasisPoints: z.coerce.number().int().min(0).max(10_000),
  mismatchBasisPoints: z.coerce.number().int().min(0).max(10_000),
  wiringBasisPoints: z.coerce.number().int().min(0).max(10_000),
  unavailabilityBasisPoints: z.coerce.number().int().min(0).max(10_000),
  calibrationBasisPoints: z.coerce.number().int().min(-5000).max(5000),
});
type FormValues = z.input<typeof formSchema>;

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
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: defaults(versioned),
  });
  useEffect(() => form.reset(defaults(versioned)), [form, versioned]);
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
    const parsed = formSchema.parse(values);
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
  const fields = [
    "soilingBasisPoints",
    "shadingBasisPoints",
    "mismatchBasisPoints",
    "wiringBasisPoints",
    "unavailabilityBasisPoints",
    "calibrationBasisPoints",
  ] as const;
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("forecasting.settings.title")}</CardTitle>
        <CardDescription>
          {t("forecasting.settings.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        <form className="grid gap-4 md:grid-cols-2" onSubmit={submit}>
          <Field>
            <FieldLabel htmlFor="forecast-effective-from">
              {t("forecasting.settings.effectiveFrom")}
            </FieldLabel>
            <Input
              id="forecast-effective-from"
              type="number"
              {...form.register("effectiveFrom")}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="forecast-effective-to">
              {t("forecasting.settings.effectiveTo")}
            </FieldLabel>
            <Input
              id="forecast-effective-to"
              type="number"
              {...form.register("effectiveTo")}
            />
          </Field>
          <Field>
            <FieldLabel htmlFor="forecast-model">
              {t("forecasting.settings.model")}
            </FieldLabel>
            <Input id="forecast-model" {...form.register("modelIdentifier")} />
          </Field>
          <Field>
            <FieldLabel htmlFor="forecast-revision">
              {t("forecasting.settings.revision")}
            </FieldLabel>
            <Input
              id="forecast-revision"
              type="number"
              {...form.register("modelRevision")}
            />
          </Field>
          {fields.map((name) => (
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
            {update.isSuccess ? (
              <span className="text-sm">{t("forecasting.settings.saved")}</span>
            ) : null}
            {update.isError ? (
              <span className="text-sm text-destructive" role="alert">
                {t("forecasting.settings.saveError")}
              </span>
            ) : null}
          </div>
        </form>
      </CardContent>
    </Card>
  );
}

function defaults(versioned?: VersionedForecastSettings): FormValues {
  const settings = versioned?.settings;
  return {
    effectiveFrom: settings?.effectiveFrom ?? Date.now(),
    effectiveTo: settings?.effectiveTo ?? "",
    modelIdentifier: settings?.modelIdentifier ?? "pvwatts-compatible",
    modelRevision: settings?.modelRevision ?? 1,
    soilingBasisPoints: settings?.losses.soilingBasisPoints ?? 0,
    shadingBasisPoints: settings?.losses.shadingBasisPoints ?? 0,
    mismatchBasisPoints: settings?.losses.mismatchBasisPoints ?? 0,
    wiringBasisPoints: settings?.losses.wiringBasisPoints ?? 0,
    unavailabilityBasisPoints: settings?.losses.unavailabilityBasisPoints ?? 0,
    calibrationBasisPoints: settings?.calibrationBasisPoints ?? 0,
  };
}
