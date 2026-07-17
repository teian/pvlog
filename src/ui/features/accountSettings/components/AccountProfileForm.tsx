import { LoadedProfileForm } from "@/features/accountSettings/components/LoadedProfileForm";
import { useAccountProfile } from "@/features/accountSettings/hooks/useAccountSettings";
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

/** Edits the current user's display name while presenting the login email as read-only. @returns Profile settings card. */
export function AccountProfileForm() {
  const { t } = useTranslation();
  const profile = useAccountProfile();

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("account.profile.title")}</CardTitle>
        <CardDescription>{t("account.profile.description")}</CardDescription>
      </CardHeader>
      <CardContent>
        {profile.isPending ? <Skeleton className="h-32 w-full" /> : null}
        {profile.isError ? (
          <Alert variant="destructive">
            <AlertTitle>{t("account.profile.loadErrorTitle")}</AlertTitle>
            <AlertDescription>
              {t("account.profile.loadErrorDescription")}
            </AlertDescription>
          </Alert>
        ) : null}
        {profile.data ? <LoadedProfileForm profile={profile.data} /> : null}
      </CardContent>
    </Card>
  );
}
