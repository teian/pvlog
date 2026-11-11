import { useSystem } from "@/entities/system";
import { SystemGeneralForm } from "@/features/systemSettings/components/SystemGeneralForm";
import { SystemLifecycleActions } from "@/features/systemSettings/components/SystemLifecycleActions";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Loads a system and shows its general settings. @param props - The system and whether it may only be viewed. @returns Loading, error or the general settings with the lifecycle actions. */
export function SystemGeneralSettings({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const system = useSystem(systemId);
  if (system.isPending) return <Skeleton className="h-96 w-full" />;
  if (system.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("systemSettings.loadErrorTitle")}</AlertTitle>
        <AlertDescription>
          {t("systemSettings.loadErrorDescription")}
        </AlertDescription>
      </Alert>
    );
  return (
    <div className="flex flex-col gap-8">
      <SystemGeneralForm readOnly={readOnly} record={system.data} />
      {readOnly ? null : <SystemLifecycleActions record={system.data} />}
    </div>
  );
}
