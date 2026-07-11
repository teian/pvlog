import { useCreateRole } from "@/features/administration/hooks/useAdministration";
import {
  Button,
  Field,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

const permissions = [
  "instance_read",
  "instance_manage",
  "account_read",
  "account_manage",
  "membership_manage",
  "role_manage",
  "system_read",
  "system_manage",
  "telemetry_read",
  "telemetry_write",
  "credential_manage",
  "integration_manage",
  "audit_read",
];

/** Creates a constrained custom role for an authorized active account. @param props - Active account context. @returns The role creation form when an account is active. */
export function CreateRoleForm({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [selected, setSelected] = useState<string[]>([]);
  const createRole = useCreateRole(accountId);
  if (!accountId) return null;

  function toggle(permission: string, checked: boolean): void {
    setSelected((current) =>
      checked
        ? [...current, permission]
        : current.filter((item) => item !== permission),
    );
  }

  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    createRole.mutate(
      { name, permissions: selected },
      {
        onSuccess: () => {
          setName("");
          setSelected([]);
        },
      },
    );
  }

  return (
    <form className="mt-6 space-y-4 border-t pt-6" onSubmit={submit}>
      <h3 className="font-medium">{t("administration.roles.createTitle")}</h3>
      <FieldGroup>
        <Field>
          <FieldLabel htmlFor="role-name">
            {t("administration.roles.name")}
          </FieldLabel>
          <Input
            id="role-name"
            onChange={(event) => {
              setName(event.target.value);
            }}
            required
            value={name}
          />
        </Field>
      </FieldGroup>
      <fieldset>
        <legend className="text-sm font-medium">
          {t("administration.roles.selectPermissions")}
        </legend>
        <div className="mt-2 grid gap-2 sm:grid-cols-2">
          {permissions.map((permission) => (
            <label className="flex items-center gap-2 text-sm" key={permission}>
              <input
                checked={selected.includes(permission)}
                onChange={(event) => {
                  toggle(permission, event.target.checked);
                }}
                type="checkbox"
              />
              {t(`administration.permissions.${permission}`)}
            </label>
          ))}
        </div>
      </fieldset>
      {createRole.isError ? (
        <p className="text-sm text-destructive">
          {t("administration.roles.createError")}
        </p>
      ) : null}
      <Button
        disabled={createRole.isPending || !name || selected.length === 0}
        type="submit"
      >
        {t("administration.roles.create")}
      </Button>
    </form>
  );
}
