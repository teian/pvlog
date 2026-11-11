import { SystemEquipmentForm } from "@/features/systemManagement/components/SystemEquipmentForm";
import {
  useInverters,
  useSaveInverters,
} from "@/features/systemManagement/hooks/useSystemManagement";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Loads a system's equipment and shows its editor. @param props - The system and whether its equipment may only be viewed. @returns The equipment section of the system settings. */
export function SystemEquipmentEditor({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const inverters = useInverters(systemId);
  const save = useSaveInverters(systemId);
  if (inverters.isPending) return <Skeleton className="h-64 w-full" />;
  if (inverters.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("equipment.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("equipment.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );
  return (
    // Remounting on new server data rebuilds the draft with the stored
    // identifiers, so a second save updates instead of creating again.
    <SystemEquipmentForm
      inverters={inverters.data}
      key={inverters.dataUpdatedAt}
      onSave={(drafts) => {
        save.mutate({ drafts, current: inverters.data });
      }}
      readOnly={readOnly}
      save={{
        pending: save.isPending,
        failed: save.isError,
        succeeded: save.isSuccess,
      }}
    />
  );
}
