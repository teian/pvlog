import {
  useAssignRole,
  useUserRoleAssignments,
} from "@/features/administration/hooks/useAdministration";
import type { Role } from "@/features/administration/types/administration.types";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Assigns a real account or instance role to one user from the table row. @param props - Scope, user and available roles. @returns A compact role selector. */
export function UserRoleSelect({
  accountId,
  roles,
  userId,
}: {
  accountId: string | null | undefined;
  roles: Role[];
  userId: string;
}) {
  const { t } = useTranslation();
  const assignment = useAssignRole(accountId);
  const assignments = useUserRoleAssignments(accountId, userId);
  const [selectedRoleId, setSelectedRoleId] = useState<string | null>(null);
  const currentRoleId =
    selectedRoleId ??
    assignments.data?.find((current) => current.systemId === null)?.roleId ??
    "";
  return (
    <div>
      <select
        aria-invalid={assignment.isError}
        aria-label={t("administration.users.roleLabel")}
        className="h-9 w-full rounded-md border border-input bg-background px-3 text-sm font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
        disabled={assignment.isPending || roles.length === 0}
        onChange={(event) => {
          const selectedRoleId = event.target.value;
          setSelectedRoleId(selectedRoleId);
          if (selectedRoleId)
            assignment.mutate({
              roleId: selectedRoleId,
              principalType: "user",
              principalId: userId,
            });
        }}
        value={currentRoleId}
      >
        <option value="">{t("administration.users.selectRole")}</option>
        {roles.map((role) => (
          <option key={role.id} value={role.id}>
            {role.kind === "built_in:InstanceAdministrator"
              ? t("administration.users.instanceAdministrator")
              : role.name}
          </option>
        ))}
      </select>
      {assignment.isError ? (
        <p className="mt-1 text-xs text-destructive" role="alert">
          {t("administration.assignments.error")}
        </p>
      ) : null}
    </div>
  );
}
