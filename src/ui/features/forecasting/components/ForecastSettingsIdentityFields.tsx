import type { ForecastSettingsFormValues } from "@/features/forecasting/utils/forecastSettingsForm";
import { Field, FieldLabel, Input } from "@/shared/components";
import type { UseFormRegister } from "react-hook-form";
import { useTranslation } from "react-i18next";

/** Forecast settings identity field properties. */
export interface ForecastSettingsIdentityFieldsProps {
  /** Field registration function of the settings form. */ register: UseFormRegister<ForecastSettingsFormValues>;
}

/** Renders the effective-dating and model-identity inputs of the forecast settings editor. @param props - Form field registration. @returns The identity fields. */
export function ForecastSettingsIdentityFields({
  register,
}: ForecastSettingsIdentityFieldsProps) {
  const { t } = useTranslation();
  return (
    <>
      <Field>
        <FieldLabel htmlFor="forecast-effective-from">
          {t("forecasting.settings.effectiveFrom")}
        </FieldLabel>
        <Input
          id="forecast-effective-from"
          type="number"
          {...register("effectiveFrom")}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="forecast-effective-to">
          {t("forecasting.settings.effectiveTo")}
        </FieldLabel>
        <Input
          id="forecast-effective-to"
          type="number"
          {...register("effectiveTo")}
        />
      </Field>
      <Field>
        <FieldLabel htmlFor="forecast-model">
          {t("forecasting.settings.model")}
        </FieldLabel>
        <Input id="forecast-model" {...register("modelIdentifier")} />
      </Field>
      <Field>
        <FieldLabel htmlFor="forecast-revision">
          {t("forecasting.settings.revision")}
        </FieldLabel>
        <Input
          id="forecast-revision"
          type="number"
          {...register("modelRevision")}
        />
      </Field>
    </>
  );
}
