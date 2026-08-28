import { EquipmentSaveStatus } from "@/features/equipmentCatalog/components/EquipmentSaveStatus";
import { InverterCatalogSelector } from "@/features/equipmentCatalog/components/InverterCatalogSelector";
import { SnapshotEditor } from "@/features/equipmentCatalog/components/SnapshotEditor";
import { SolarModuleCatalogSelector } from "@/features/equipmentCatalog/components/SolarModuleCatalogSelector";
import { useSaveEquipmentConfiguration } from "@/features/equipmentCatalog/hooks/useEquipmentCatalog";
import type {
  InverterCatalogEntry,
  SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import {
  buildEquipmentSaveRequest,
  snapshot,
} from "@/features/equipmentCatalog/utils/equipmentSnapshot";
import { Button, Input, Label } from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Persists manually entered or fully editable catalog-prefilled snapshots. @param props - Active account and system. @returns The confirmation form. */
export function EquipmentConfirmationForm({
  accountId,
  systemId,
}: {
  accountId: string;
  systemId: string;
}) {
  const { t } = useTranslation();
  const save = useSaveEquipmentConfiguration(accountId, systemId);
  const [inverter, setInverter] = useState<InverterCatalogEntry | null>(null);
  const [module, setModule] = useState<SolarModuleCatalogEntry | null>(null);
  const [inverterJson, setInverterJson] = useState("");
  const [moduleJson, setModuleJson] = useState("");
  const [count, setCount] = useState(1);
  const chooseInverter = (entry: InverterCatalogEntry) => {
    setInverter(entry);
    setInverterJson(JSON.stringify(snapshot(entry), null, 2));
  };
  const chooseModule = (entry: SolarModuleCatalogEntry) => {
    setModule(entry);
    setModuleJson(JSON.stringify(snapshot(entry), null, 2));
  };
  const submit = () => {
    try {
      save.mutate(
        buildEquipmentSaveRequest({
          count,
          inverter,
          inverterJson,
          module,
          moduleJson,
          t,
        }),
      );
    } catch {
      save.reset();
    }
  };
  return (
    <section aria-labelledby="equipment-confirm-title" className="space-y-4">
      <h3 className="text-sm font-semibold" id="equipment-confirm-title">
        {t("features.equipmentCatalog.confirmTitle")}
      </h3>
      <div className="grid gap-6 lg:grid-cols-2">
        <InverterCatalogSelector
          onManual={() => {
            setInverter(null);
            setInverterJson("");
          }}
          onSelect={chooseInverter}
        />
        <SolarModuleCatalogSelector
          onManual={() => {
            setModule(null);
            setModuleJson("");
          }}
          onSelect={chooseModule}
        />
      </div>
      <SnapshotEditor
        id="confirmed-inverter-json"
        label={t("features.equipmentCatalog.inverterSnapshot")}
        onChange={setInverterJson}
        value={inverterJson}
      />
      <SnapshotEditor
        id="confirmed-module-json"
        label={t("features.equipmentCatalog.moduleSnapshot")}
        onChange={setModuleJson}
        value={moduleJson}
      />
      <div className="space-y-2">
        <Label htmlFor="confirmed-module-count">
          {t("features.equipmentCatalog.moduleCount")}
        </Label>
        <Input
          id="confirmed-module-count"
          min={1}
          onChange={(event) => {
            setCount(Number(event.target.value));
          }}
          type="number"
          value={count}
        />
      </div>
      <p className="text-xs text-muted-foreground">
        {t("features.equipmentCatalog.snapshotGuidance")}
      </p>
      <p className="text-xs text-muted-foreground">
        {t("features.equipmentCatalog.provenanceLegend")}
      </p>
      <Button disabled={save.isPending} onClick={submit} type="button">
        {t("features.equipmentCatalog.save")}
      </Button>
      <EquipmentSaveStatus error={save.isError} success={save.isSuccess} />
    </section>
  );
}
