import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/shared/components";
import { PlusIcon } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";

/**
 * Explains that the account has no system yet, in place of an empty view.
 *
 * @param props - Whether the session may add a system.
 * @returns The explanation, with the way to add a system when permitted.
 */
export function NoSystemNotice({ canManage }: { canManage: boolean }) {
  const { t } = useTranslation();
  return (
    <Card>
      <CardHeader>
        <CardTitle>
          <h2>{t("system.noSystem.title")}</h2>
        </CardTitle>
        <CardDescription>
          {t(
            canManage
              ? "system.noSystem.description"
              : "system.noSystem.descriptionReadOnly",
          )}
        </CardDescription>
      </CardHeader>
      {canManage ? (
        <CardContent>
          <Button asChild>
            <Link to="/setup">
              <PlusIcon data-icon="inline-start" />
              {t("system.noSystem.add")}
            </Link>
          </Button>
        </CardContent>
      ) : null}
    </Card>
  );
}
