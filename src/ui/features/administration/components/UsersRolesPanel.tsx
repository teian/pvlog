import {
  useAdministrationUsers,
  useRoles,
} from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";
import { DeleteUserButton } from "./DeleteUserButton";
import { InviteUserDialog } from "./InviteUserDialog";
import { UserRoleSelect } from "./UserRoleSelect";

const ROW_GRID =
  "md:grid md:grid-cols-[minmax(15rem,1.55fr)_minmax(12rem,1fr)_minmax(8rem,0.7fr)_2.75rem] md:items-center md:gap-3";

/** Matches the administration mock with one responsive, server-backed users-and-roles table. @param props - Active account, or no account for instance-scoped roles. @returns The user administration card. */
export function UsersRolesPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const users = useAdministrationUsers();
  const roles = useRoles(accountId);
  return (
    <Card className="gap-0 overflow-hidden py-0">
      <CardHeader className="flex-row items-center justify-between gap-4 border-b px-5 py-4">
        <div className="min-w-0">
          <CardTitle>
            <h2>{t("administration.users.title")}</h2>
          </CardTitle>
          <CardDescription>
            {t("administration.users.description")}
          </CardDescription>
        </div>
        <InviteUserDialog />
      </CardHeader>
      <CardContent className="px-5 py-0">
        {users.isLoading || roles.isLoading ? (
          <Skeleton className="my-5 h-32" />
        ) : null}
        {users.isError ? (
          <p className="py-5 text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {users.data ? (
          <div>
            <div
              aria-hidden="true"
              className={`${ROW_GRID} hidden border-b py-3 text-[10px] font-semibold uppercase tracking-widest text-muted-foreground`}
            >
              <span>{t("administration.users.columns.user")}</span>
              <span>{t("administration.users.columns.role")}</span>
              <span>{t("administration.users.columns.status")}</span>
              <span />
            </div>
            <ul aria-label={t("administration.users.title")}>
              {users.data.map((user) => (
                <li
                  className={`${ROW_GRID} space-y-3 border-b py-3 last:border-b-0 md:space-y-0`}
                  key={user.id}
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-semibold">
                      {user.displayName}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {user.email}
                    </p>
                  </div>
                  <UserRoleSelect
                    accountId={accountId}
                    roles={roles.data ?? []}
                    userId={user.id}
                  />
                  <div className="flex items-center gap-2 text-xs font-semibold uppercase text-muted-foreground">
                    <span
                      aria-hidden="true"
                      className={
                        user.status === "active"
                          ? "size-1.5 rounded-full bg-success"
                          : user.status === "invited"
                            ? "size-1.5 rounded-full bg-warning"
                            : "size-1.5 rounded-full bg-muted-foreground"
                      }
                    />
                    {t(`administration.users.status.${user.status}`)}
                  </div>
                  <div className="flex justify-end">
                    <DeleteUserButton
                      displayName={user.displayName}
                      userId={user.id}
                    />
                  </div>
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </CardContent>
    </Card>
  );
}
