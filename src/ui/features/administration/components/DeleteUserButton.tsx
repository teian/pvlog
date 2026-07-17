import { useDeleteAdministrationUser } from "@/features/administration/hooks/useAdministration";
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
  Button,
} from "@/shared/components";
import { Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";

/** Deletes one eligible local user after explicit confirmation. @param props - User identity shown in the confirmation. @returns A destructive icon action. */
export function DeleteUserButton({
  displayName,
  userId,
}: {
  displayName: string;
  userId: string;
}) {
  const { t } = useTranslation();
  const deletion = useDeleteAdministrationUser();
  return (
    <AlertDialog>
      <AlertDialogTrigger asChild>
        <Button
          aria-label={t("administration.users.deleteLabel", { displayName })}
          className="size-8 text-destructive hover:bg-destructive/10 hover:text-destructive"
          disabled={deletion.isPending}
          size="icon"
          variant="outline"
        >
          <Trash2 aria-hidden="true" className="size-4" />
        </Button>
      </AlertDialogTrigger>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {t("administration.users.deleteTitle")}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {t("administration.users.deleteDescription", { displayName })}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>
            {t("administration.users.cancelDelete")}
          </AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              deletion.mutate(userId);
            }}
            variant="destructive"
          >
            {t("administration.users.confirmDelete")}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
