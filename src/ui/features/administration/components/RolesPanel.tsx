import { useRoles } from "@/features/administration/hooks/useAdministration";
import { CreateRoleForm } from "@/features/administration/components/CreateRoleForm";
import { RoleAssignmentForm } from "@/features/administration/components/RoleAssignmentForm";
import {
  Badge,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders roles when the server grants role-management access. @param props - Active account context. @returns The role panel. */
export function RolesPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const roles = useRoles(accountId);
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.roles.title")}</CardTitle>
        <CardDescription>
          {t("administration.roles.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {roles.isLoading ? <Skeleton className="h-16 w-full" /> : null}
        {roles.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {roles.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.roles.empty")}
          </p>
        ) : null}
        <ul className="space-y-3" aria-label={t("administration.roles.title")}>
          {roles.data?.map((role) => (
            <li className="rounded-md border p-3" key={role.id}>
              <div className="flex flex-wrap items-center gap-2">
                <p className="font-medium">{role.name}</p>
                <Badge variant="secondary">
                  {t(`administration.roles.kinds.${role.kind}`, {
                    defaultValue: role.kind,
                  })}
                </Badge>
              </div>
              <p className="mt-1 text-sm text-muted-foreground">
                {t("administration.roles.permissions", {
                  count: role.permissions.length,
                })}
              </p>
            </li>
          ))}
        </ul>
        {roles.data ? <CreateRoleForm accountId={accountId} /> : null}
        {roles.data ? (
          <RoleAssignmentForm accountId={accountId} roles={roles.data} />
        ) : null}
      </CardContent>
    </Card>
  );
}
