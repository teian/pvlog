import type { Role } from "@/features/administration/types/administration.types";
import { FieldLabel, Input } from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders typed principal and scope fields for a role assignment. @param props - Selected role and field values. @returns The assignment fields. */
export function RoleAssignmentFields({
  roles,
  roleId,
  principalType,
  principalId,
  systemId,
  onRoleIdChange,
  onPrincipalTypeChange,
  onPrincipalIdChange,
  onSystemIdChange,
}: {
  roles: Role[];
  roleId: string;
  principalType: "user" | "api_credential";
  principalId: string;
  systemId: string;
  onRoleIdChange: (value: string) => void;
  onPrincipalTypeChange: (value: "user" | "api_credential") => void;
  onPrincipalIdChange: (value: string) => void;
  onSystemIdChange: (value: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className="grid gap-4">
      <div>
        <FieldLabel htmlFor="assignment-role">
          {t("administration.assignments.role")}
        </FieldLabel>
        <select
          className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
          id="assignment-role"
          onChange={(event) => {
            onRoleIdChange(event.target.value);
          }}
          value={roleId}
        >
          {roles.map((role) => (
            <option key={role.id} value={role.id}>
              {role.name}
            </option>
          ))}
        </select>
      </div>
      <div>
        <FieldLabel htmlFor="assignment-principal-type">
          {t("administration.assignments.principalType")}
        </FieldLabel>
        <select
          className="h-9 rounded-md border border-input bg-transparent px-3 text-sm"
          id="assignment-principal-type"
          onChange={(event) => {
            onPrincipalTypeChange(
              event.target.value as "user" | "api_credential",
            );
          }}
          value={principalType}
        >
          <option value="user">{t("administration.assignments.user")}</option>
          <option value="api_credential">
            {t("administration.assignments.apiCredential")}
          </option>
        </select>
      </div>
      <div>
        <FieldLabel htmlFor="assignment-principal-id">
          {t("administration.assignments.principalId")}
        </FieldLabel>
        <Input
          id="assignment-principal-id"
          onChange={(event) => {
            onPrincipalIdChange(event.target.value);
          }}
          required
          value={principalId}
        />
      </div>
      <div>
        <FieldLabel htmlFor="assignment-system-id">
          {t("administration.assignments.systemId")}
        </FieldLabel>
        <Input
          id="assignment-system-id"
          onChange={(event) => {
            onSystemIdChange(event.target.value);
          }}
          value={systemId}
        />
      </div>
    </div>
  );
}
