/* eslint-disable max-lines-per-function -- one form over the six values of a tariff with shared validation */
import type { Tariff, TariffInput } from "@/features/tariffs/tariffs.types";
import {
  Button,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

const dateOf = (epochMillis: number | null) =>
  epochMillis === null ? "" : new Date(epochMillis).toISOString().slice(0, 10);

const millisOf = (date: string) => new Date(`${date}T00:00:00Z`).getTime();

/** Creates or edits one tariff. @param props - The tariff being edited (absent to create one), request state, and the submit and cancel callbacks. @returns A form that names what is missing before it submits. */
export function TariffForm({
  tariff,
  pending,
  onSubmit,
  onCancel,
}: {
  tariff?: Tariff;
  pending: boolean;
  onSubmit: (input: TariffInput) => void;
  onCancel?: () => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState(tariff?.name ?? "");
  const [direction, setDirection] = useState(tariff?.direction ?? "export");
  const [currency, setCurrency] = useState(tariff?.currencyCode ?? "EUR");
  const [price, setPrice] = useState(
    tariff ? String(tariff.minorUnitsPerKwh) : "",
  );
  const [from, setFrom] = useState(
    dateOf(tariff?.effectiveFromEpochMillis ?? null),
  );
  const [to, setTo] = useState(dateOf(tariff?.effectiveToEpochMillis ?? null));
  const [attempted, setAttempted] = useState(false);
  const priceValue = Number(price.trim());
  const valid =
    name.trim() !== "" &&
    /^[A-Za-z]{3}$/u.test(currency.trim()) &&
    price.trim() !== "" &&
    Number.isInteger(priceValue) &&
    priceValue >= 0 &&
    from !== "" &&
    (to === "" || to > from);
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (!valid) return;
    onSubmit({
      name: name.trim(),
      direction,
      currencyCode: currency.trim().toUpperCase(),
      minorUnitsPerKwh: priceValue,
      effectiveFromEpochMillis: millisOf(from),
      effectiveToEpochMillis: to === "" ? null : millisOf(to),
    });
  }
  const prefix = tariff ? `tariff-${tariff.id}` : "tariff-new";
  return (
    <form
      className="flex flex-col gap-4 rounded-md border p-4"
      onSubmit={submit}
    >
      <FieldGroup className="grid gap-3 sm:grid-cols-3">
        <Field>
          <FieldLabel htmlFor={`${prefix}-name`}>
            {t("tariffs.name")}
          </FieldLabel>
          <Input
            id={`${prefix}-name`}
            onChange={(event) => {
              setName(event.target.value);
            }}
            value={name}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-direction`}>
            {t("tariffs.direction")}
          </FieldLabel>
          <select
            className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
            id={`${prefix}-direction`}
            onChange={(event) => {
              setDirection(
                event.target.value === "import" ? "import" : "export",
              );
            }}
            value={direction}
          >
            <option value="export">{t("tariffs.directions.export")}</option>
            <option value="import">{t("tariffs.directions.import")}</option>
          </select>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-currency`}>
            {t("tariffs.currency")}
          </FieldLabel>
          <Input
            id={`${prefix}-currency`}
            maxLength={3}
            onChange={(event) => {
              setCurrency(event.target.value);
            }}
            value={currency}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-price`}>
            {t("tariffs.price")}
          </FieldLabel>
          <Input
            aria-describedby={`${prefix}-price-hint`}
            id={`${prefix}-price`}
            inputMode="numeric"
            onChange={(event) => {
              setPrice(event.target.value);
            }}
            value={price}
          />
          <FieldDescription id={`${prefix}-price-hint`}>
            {t("tariffs.priceHint")}
          </FieldDescription>
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-from`}>
            {t("tariffs.from")}
          </FieldLabel>
          <Input
            id={`${prefix}-from`}
            onChange={(event) => {
              setFrom(event.target.value);
            }}
            type="date"
            value={from}
          />
        </Field>
        <Field>
          <FieldLabel htmlFor={`${prefix}-to`}>{t("tariffs.to")}</FieldLabel>
          <Input
            id={`${prefix}-to`}
            onChange={(event) => {
              setTo(event.target.value);
            }}
            type="date"
            value={to}
          />
        </Field>
      </FieldGroup>
      {attempted && !valid ? (
        <p className="text-sm text-destructive" role="alert">
          {t("tariffs.invalid")}
        </p>
      ) : null}
      <div className="flex flex-wrap gap-3">
        <Button disabled={pending} type="submit">
          {t(tariff ? "tariffs.save" : "tariffs.add")}
        </Button>
        {onCancel ? (
          <Button onClick={onCancel} type="button" variant="outline">
            {t("dataSources.cancel")}
          </Button>
        ) : null}
      </div>
    </form>
  );
}
