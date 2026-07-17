import {
  accountApiKeyScopeKey,
  type AccountApiKey,
} from "@/features/accountApiKeys/types/accountApiKeys.types";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  Badge,
  Button,
  TableCell,
  TableRow,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders one safe key record and its confirmed revoke action. @param props - Key metadata, date formatter, and revoke callback. @returns Account API-key row. */
export function AccountApiKeyRow({
  apiKey,
  formatDate,
  onRevoke,
}: {
  apiKey: AccountApiKey;
  formatDate: (value: number | null) => string;
  onRevoke: (id: string) => void;
}) {
  const { t } = useTranslation();
  const active = apiKey.revokedAtEpochMillis === null;
  return (
    <TableRow>
      <TableCell className="font-medium">{apiKey.name}</TableCell>
      <TableCell>
        <div className="flex flex-wrap gap-1">
          {apiKey.scopes.map((scope) => (
            <Badge key={scope} variant="outline">
              {t(`accountApiKeys.scopes.${accountApiKeyScopeKey(scope)}.short`)}
            </Badge>
          ))}
        </div>
      </TableCell>
      <TableCell>{formatDate(apiKey.createdAtEpochMillis)}</TableCell>
      <TableCell>{formatDate(apiKey.expiresAtEpochMillis)}</TableCell>
      <TableCell>
        <Badge variant={active ? "secondary" : "outline"}>
          {t(active ? "accountApiKeys.active" : "accountApiKeys.revoked")}
        </Badge>
      </TableCell>
      <TableCell>
        {active ? (
          <AlertDialog>
            <AlertDialogTrigger asChild>
              <Button size="sm" variant="outline">
                {t("accountApiKeys.revoke")}
              </Button>
            </AlertDialogTrigger>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>
                  {t("accountApiKeys.revokeTitle", { name: apiKey.name })}
                </AlertDialogTitle>
                <AlertDialogDescription>
                  {t("accountApiKeys.revokeDescription")}
                </AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel>
                  {t("accountApiKeys.cancel")}
                </AlertDialogCancel>
                <AlertDialogAction
                  onClick={() => {
                    onRevoke(apiKey.id);
                  }}
                  variant="destructive"
                >
                  {t("accountApiKeys.confirmRevoke")}
                </AlertDialogAction>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        ) : null}
      </TableCell>
    </TableRow>
  );
}
