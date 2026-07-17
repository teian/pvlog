import { useAdministrationUsers } from "@/features/administration/hooks/useAdministration";
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

/** Displays the administrator-visible local user directory without credentials. */
export function UserDirectoryPanel() {
  const { t } = useTranslation();
  const users = useAdministrationUsers();
  return (
    <Card className="gap-0 overflow-hidden py-0">
      <CardHeader className="border-b py-5">
        <CardTitle>{t("administration.users.title")}</CardTitle>
        <CardDescription>
          {t("administration.users.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="px-0">
        {users.isLoading ? <Skeleton className="m-5 h-20" /> : null}
        {users.isError ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        <ul aria-label={t("administration.users.title")}>
          {users.data?.map((user) => (
            <li
              className="flex items-center gap-4 border-b px-5 py-3 last:border-b-0"
              key={user.id}
            >
              <span className="min-w-0 flex-1">
                <span className="block text-sm font-semibold">
                  {user.displayName}
                </span>
                <span className="block truncate text-sm text-muted-foreground">
                  {user.email}
                </span>
              </span>
              <Badge
                variant={user.status === "active" ? "secondary" : "outline"}
              >
                {t(`administration.users.status.${user.status}`)}
              </Badge>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
