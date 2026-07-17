import { useSession } from "@/features/auth";
import { SystemManagementCard } from "@/features/systemManagement/components/SystemManagementCard";
import { SystemWizard } from "@/features/systemManagement/components/SystemWizard";
import {
  useDeleteManagedSystem,
  useManagedSystems,
  useSaveManagedSystem,
} from "@/features/systemManagement/hooks/useSystemManagement";
import type { ManagedSystem } from "@/features/systemManagement/types/systemManagement.types";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Button,
  Skeleton,
} from "@/shared/components";
import { PlusIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Owns real system-management queries and create/edit/list transitions. @returns Management list or single-page wizard. */
export function SystemManagementView() {
  const { t } = useTranslation();
  const session = useSession();
  const systemIds = session.data?.systemIds ?? [];
  const queries = useManagedSystems(systemIds);
  const systems = queries.flatMap((query) => (query.data ? [query.data] : []));
  const failed = queries.some((query) => query.isError);
  const pending = queries.some((query) => query.isPending);
  const save = useSaveManagedSystem();
  const remove = useDeleteManagedSystem();
  const [editing, setEditing] = useState<ManagedSystem | null | undefined>();
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  if (editing !== undefined)
    return (
      <SystemWizard
        {...(editing ? { current: editing } : {})}
        error={save.error}
        onCancel={() => {
          setEditing(undefined);
          save.reset();
        }}
        onSubmit={async (draft) => {
          const record = await save.mutateAsync({
            draft,
            ...(editing ? { current: editing } : {}),
          });
          setExpanded((current) => ({ ...current, [record.id]: true }));
        }}
        pending={save.isPending}
      />
    );
  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col items-start justify-between gap-3 sm:flex-row sm:items-center">
        <p className="text-xs text-muted-foreground">
          {t("systemManagement.description")}
        </p>
        <Button
          onClick={() => {
            setEditing(null);
          }}
          size="sm"
        >
          <PlusIcon data-icon="inline-start" />
          {t("systemManagement.actions.addSystem")}
        </Button>
      </div>
      {failed ? (
        <Alert variant="destructive">
          <AlertTitle>{t("systemManagement.errors.loadTitle")}</AlertTitle>
          <AlertDescription>
            {t("systemManagement.errors.loadDescription")}
          </AlertDescription>
        </Alert>
      ) : null}
      {pending ? (
        <div className="flex flex-col gap-3">
          <Skeleton className="h-32 w-full" />
          <Skeleton className="h-32 w-full" />
        </div>
      ) : null}
      {!pending && systems.length === 0 ? (
        <Alert>
          <AlertTitle>{t("systemManagement.empty.title")}</AlertTitle>
          <AlertDescription>
            {t("systemManagement.empty.description")}
          </AlertDescription>
        </Alert>
      ) : null}
      <div className="flex flex-col gap-3.5">
        {systems.map((system, index) => (
          <SystemManagementCard
            canDelete={systems.length > 1}
            expanded={expanded[system.record.id] ?? index === 0}
            key={system.record.id}
            onDelete={() => {
              remove.mutate(system);
            }}
            onEdit={() => {
              setEditing(system);
            }}
            onToggle={() => {
              setExpanded((current) => ({
                ...current,
                [system.record.id]: !(current[system.record.id] ?? index === 0),
              }));
            }}
            system={system}
          />
        ))}
      </div>
    </div>
  );
}
