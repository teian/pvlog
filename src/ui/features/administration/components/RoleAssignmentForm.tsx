import { useAssignRole } from "@/features/administration/hooks/useAdministration";
import type { Role } from "@/features/administration/types/administration.types";
import { Button } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";
import { RoleAssignmentFields } from "./RoleAssignmentFields";

/** Assigns an existing role to a typed principal at account or optional system scope. @param props - Active account and available roles. @returns The assignment form. */
export function RoleAssignmentForm({
  accountId,
  roles,
}: {
  accountId: string | null | undefined;
  roles: Role[];
}) {
  const { t } = useTranslation();
  const [roleId, setRoleId] = useState(roles[0]?.id ?? "");
  const [principalType, setPrincipalType] = useState<"user" | "api_credential">(
    "user",
  );
  const [principalId, setPrincipalId] = useState("");
  const [systemId, setSystemId] = useState("");
  const assignRole = useAssignRole(accountId);
  if (!accountId || roles.length === 0) return null;
  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    assignRole.mutate({
      roleId,
      principalType,
      principalId,
      ...(systemId ? { systemId } : {}),
    });
  }
  return (
    <form className="mt-6 space-y-4 border-t pt-6" onSubmit={submit}>
      <h3 className="font-medium">{t("administration.assignments.title")}</h3>
      <RoleAssignmentFields
        onPrincipalIdChange={setPrincipalId}
        onPrincipalTypeChange={setPrincipalType}
        onRoleIdChange={setRoleId}
        onSystemIdChange={setSystemId}
        principalId={principalId}
        principalType={principalType}
        roleId={roleId}
        roles={roles}
        systemId={systemId}
      />
      {assignRole.isError ? (
        <p className="text-sm text-destructive">
          {t("administration.assignments.error")}
        </p>
      ) : null}
      <Button
        disabled={assignRole.isPending || !roleId || !principalId}
        type="submit"
      >
        {t("administration.assignments.create")}
      </Button>
    </form>
  );
}
