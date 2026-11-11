import { Alert, AlertDescription, AlertTitle } from "@/shared/components";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/** Points from the former place of system equipment to where it is edited now. @param props - A system to link to, when the session has one. @returns A notice with a link to the system's settings. */
export function SystemSettingsMovedNotice({
  systemId,
}: {
  systemId: string | undefined;
}) {
  const { t } = useTranslation();
  return (
    <Alert>
      <AlertTitle>{t("administration.moved.title")}</AlertTitle>
      <AlertDescription>
        {t("administration.moved.description")}{" "}
        {systemId ? (
          <Link
            className="font-semibold underline"
            to={`/systems/${systemId}/settings/equipment`}
          >
            {t("administration.moved.link")}
          </Link>
        ) : null}
      </AlertDescription>
    </Alert>
  );
}
