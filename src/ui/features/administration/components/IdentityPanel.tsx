import { useLinkedIdentities } from "@/features/administration/hooks/useAdministration";
import {
  Alert,
  AlertDescription,
  AlertTitle,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

function formatTimestamp(epochMillis: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(epochMillis));
}

/** Renders external identities for the authenticated browser user. @returns The identity panel. */
export function IdentityPanel() {
  const { i18n, t } = useTranslation();
  const identities = useLinkedIdentities();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("administration.identities.title")}</CardTitle>
        <CardDescription>
          {t("administration.identities.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {identities.isLoading ? <Skeleton className="h-16 w-full" /> : null}
        {identities.isError ? (
          <Alert variant="destructive">
            <AlertTitle>{t("administration.unavailableTitle")}</AlertTitle>
            <AlertDescription>
              {t("administration.identities.unavailable")}
            </AlertDescription>
          </Alert>
        ) : null}
        {identities.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("administration.identities.empty")}
          </p>
        ) : null}
        <ul
          className="space-y-3"
          aria-label={t("administration.identities.title")}
        >
          {identities.data?.map((identity) => (
            <li className="rounded-md border p-3" key={identity.id}>
              <p className="font-medium">{identity.subject}</p>
              <p className="text-sm text-muted-foreground">
                {t("administration.identities.linked", {
                  connector: identity.connectorId.slice(0, 8),
                  date: formatTimestamp(
                    identity.linkedAtEpochMillis,
                    i18n.language,
                  ),
                })}
              </p>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
