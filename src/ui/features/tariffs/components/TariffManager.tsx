/* eslint-disable max-lines-per-function -- the manager wires one list to its edit state and three mutations */
import { TariffForm } from "@/features/tariffs/components/TariffForm";
import {
  useDeleteTariff,
  useSaveTariff,
  useTariffs,
} from "@/features/tariffs/useTariffs";
import { SessionRequestError } from "@/shared/api/sessionRequest";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Skeleton,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Lists a system's tariffs and lets them be created, edited and deleted. @param props - The system and whether its tariffs may only be viewed. @returns The tariffs section of the system settings. */
export function TariffManager({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t, i18n } = useTranslation();
  const tariffs = useTariffs(systemId);
  const save = useSaveTariff(systemId);
  const remove = useDeleteTariff(systemId);
  const [editing, setEditing] = useState<string>();
  // A fresh form after each successful creation.
  const [created, setCreated] = useState(0);
  const date = new Intl.DateTimeFormat(i18n.language, {
    dateStyle: "medium",
    timeZone: "UTC",
  });

  if (tariffs.isPending) return <Skeleton className="h-40 w-full" />;
  if (tariffs.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("tariffs.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("tariffs.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );

  const overlap =
    save.error instanceof SessionRequestError && save.error.status === 409;
  return (
    <div className="flex flex-col gap-4">
      {tariffs.data.length === 0 ? (
        <Alert>
          <AlertTitle>{t("tariffs.empty.title")}</AlertTitle>
          <AlertDescription>{t("tariffs.empty.description")}</AlertDescription>
        </Alert>
      ) : (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>{t("tariffs.name")}</TableHead>
              <TableHead>{t("tariffs.direction")}</TableHead>
              <TableHead>{t("tariffs.price")}</TableHead>
              <TableHead>{t("tariffs.validity")}</TableHead>
              {readOnly ? null : (
                <TableHead>{t("dataSources.columns.actions")}</TableHead>
              )}
            </TableRow>
          </TableHeader>
          <TableBody>
            {tariffs.data.map((tariff) => (
              <TableRow key={tariff.id}>
                <TableCell className="font-medium">{tariff.name}</TableCell>
                <TableCell>
                  {t(`tariffs.directions.${tariff.direction}`)}
                </TableCell>
                <TableCell className="font-mono">
                  {t("tariffs.priceValue", {
                    price: tariff.minorUnitsPerKwh,
                    currency: tariff.currencyCode,
                  })}
                </TableCell>
                <TableCell>
                  {tariff.effectiveToEpochMillis === null
                    ? t("tariffs.validFrom", {
                        from: date.format(tariff.effectiveFromEpochMillis),
                      })
                    : t("tariffs.validRange", {
                        from: date.format(tariff.effectiveFromEpochMillis),
                        to: date.format(tariff.effectiveToEpochMillis),
                      })}
                </TableCell>
                {readOnly ? null : (
                  <TableCell>
                    <div className="flex flex-wrap gap-2">
                      <Button
                        aria-label={t("tariffs.editOf", { name: tariff.name })}
                        onClick={() => {
                          setEditing(tariff.id);
                        }}
                        size="sm"
                        variant="outline"
                      >
                        {t("tariffs.edit")}
                      </Button>
                      <Button
                        aria-label={t("tariffs.deleteOf", {
                          name: tariff.name,
                        })}
                        disabled={remove.isPending}
                        onClick={() => {
                          remove.mutate(tariff.id);
                        }}
                        size="sm"
                        variant="outline"
                      >
                        {t("tariffs.delete")}
                      </Button>
                    </div>
                  </TableCell>
                )}
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
      {save.isError || remove.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {overlap ? t("tariffs.errors.overlap") : t("tariffs.errors.save")}
        </p>
      ) : null}
      {readOnly
        ? null
        : tariffs.data
            .filter((tariff) => tariff.id === editing)
            .map((tariff) => (
              <TariffForm
                key={tariff.id}
                onCancel={() => {
                  setEditing(undefined);
                }}
                onSubmit={(input) => {
                  save.mutate(
                    { input, id: tariff.id },
                    {
                      onSuccess: () => {
                        setEditing(undefined);
                      },
                    },
                  );
                }}
                pending={save.isPending}
                tariff={tariff}
              />
            ))}
      {readOnly || editing ? null : (
        <TariffForm
          key={created}
          onSubmit={(input) => {
            save.mutate(
              { input },
              {
                onSuccess: () => {
                  setCreated((count) => count + 1);
                },
              },
            );
          }}
          pending={save.isPending}
        />
      )}
    </div>
  );
}
