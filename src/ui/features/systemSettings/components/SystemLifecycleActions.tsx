import {
  useDeleteSystem,
  useSetSystemArchived,
  type SystemRecord,
} from "@/entities/system";
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
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";

/** Archives, restores or permanently deletes a system. @param props - The stored system. @returns The lifecycle actions, with a confirmation before deletion. */
export function SystemLifecycleActions({ record }: { record: SystemRecord }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const archive = useSetSystemArchived(record.id);
  const remove = useDeleteSystem(record.id);
  const archived = record.lifecycle === "archived";
  const busy = archive.isPending || remove.isPending;
  return (
    <section
      aria-labelledby="system-lifecycle-title"
      className="flex flex-col gap-3 rounded-md border border-destructive/30 p-4"
    >
      <h2 className="text-sm font-bold" id="system-lifecycle-title">
        {t("systemSettings.lifecycle.title")}
      </h2>
      <p className="text-sm text-muted-foreground">
        {t(
          archived
            ? "systemSettings.lifecycle.archivedDescription"
            : "systemSettings.lifecycle.activeDescription",
        )}
      </p>
      <div className="flex flex-wrap gap-3">
        <Button
          disabled={busy}
          onClick={() => {
            archive.mutate({
              version: record.version,
              action: archived ? "restore" : "archive",
            });
          }}
          type="button"
          variant="outline"
        >
          {t(
            archived
              ? "systemSettings.lifecycle.restore"
              : "systemSettings.lifecycle.archive",
          )}
        </Button>
        <AlertDialog>
          <AlertDialogTrigger asChild>
            <Button disabled={busy} type="button" variant="destructive">
              {t("systemSettings.lifecycle.delete")}
            </Button>
          </AlertDialogTrigger>
          <AlertDialogContent>
            <AlertDialogHeader>
              <AlertDialogTitle>
                {t("systemSettings.lifecycle.deleteTitle")}
              </AlertDialogTitle>
              <AlertDialogDescription>
                {t("systemSettings.lifecycle.deleteDescription", {
                  name: record.name,
                })}
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>{t("dataSources.cancel")}</AlertDialogCancel>
              <AlertDialogAction
                onClick={() => {
                  remove.mutate(record.version, {
                    onSuccess: () => {
                      void navigate("/systems");
                    },
                  });
                }}
              >
                {t("systemSettings.lifecycle.deleteConfirm")}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      </div>
      {archive.isError || remove.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("systemSettings.general.errors.unavailable")}
        </p>
      ) : null}
    </section>
  );
}
