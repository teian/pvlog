/* eslint-disable max-lines-per-function -- visibility, the link switch and the confirmed rotation act on one sharing state */
import type { SharingState } from "@/features/sharing/sharing.types";
import { useSharing, useUpdateSharing } from "@/features/sharing/useSharing";
import {
  Alert,
  AlertDescription,
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
  AlertTitle,
  Button,
  Field,
  FieldDescription,
  FieldLabel,
  Input,
  Skeleton,
  Switch,
} from "@/shared/components";
import { CheckIcon, CopyIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

const VISIBILITIES = ["private", "unlisted", "public"] as const;

/** Builds the address under which a shared system can be read without signing in. */
function publicLink(token: string) {
  return `${window.location.origin}/api/v1/public/systems/${token}`;
}

/** Changes who can see a system and manages its public link. @param props - The system and whether its sharing may only be viewed. @returns The sharing section of the system settings. */
export function SharingSettings({
  systemId,
  readOnly,
}: {
  systemId: string;
  readOnly: boolean;
}) {
  const { t } = useTranslation();
  const sharing = useSharing(systemId);
  const update = useUpdateSharing(systemId);
  const [copied, setCopied] = useState(false);
  const [rotated, setRotated] = useState(false);

  if (sharing.isPending) return <Skeleton className="h-40 w-full" />;
  if (sharing.isError)
    return (
      <Alert variant="destructive">
        <AlertTitle>{t("sharing.errors.loadTitle")}</AlertTitle>
        <AlertDescription>
          {t("sharing.errors.loadDescription")}
        </AlertDescription>
      </Alert>
    );

  const state: SharingState = sharing.data;
  const disabled = readOnly || update.isPending;
  const link =
    state.linkEnabled && state.publicToken
      ? publicLink(state.publicToken)
      : null;
  return (
    <div className="flex flex-col gap-6">
      <Field>
        <FieldLabel htmlFor="sharing-visibility">
          {t("sharing.visibility")}
        </FieldLabel>
        <select
          aria-describedby="sharing-visibility-hint"
          className="h-9 w-fit rounded-md border border-input bg-transparent px-3 text-sm"
          disabled={disabled}
          id="sharing-visibility"
          onChange={(event) => {
            const visibility = VISIBILITIES.find(
              (value) => value === event.target.value,
            );
            if (visibility)
              update.mutate({ linkEnabled: state.linkEnabled, visibility });
          }}
          value={state.visibility}
        >
          {VISIBILITIES.map((value) => (
            <option key={value} value={value}>
              {t(`sharing.visibilities.${value}.name`)}
            </option>
          ))}
        </select>
        <FieldDescription id="sharing-visibility-hint">
          {t(`sharing.visibilities.${state.visibility}.description`)}
        </FieldDescription>
      </Field>
      <Field orientation="horizontal">
        <Switch
          checked={state.linkEnabled}
          disabled={disabled}
          id="sharing-link"
          onCheckedChange={(linkEnabled) => {
            setRotated(false);
            update.mutate({ linkEnabled });
          }}
        />
        <FieldLabel htmlFor="sharing-link">{t("sharing.link")}</FieldLabel>
      </Field>
      {link ? (
        <div className="flex flex-col gap-3">
          <Field>
            <FieldLabel htmlFor="sharing-link-value">
              {t("sharing.linkValue")}
            </FieldLabel>
            <div className="flex gap-2">
              <Input
                className="font-mono"
                id="sharing-link-value"
                readOnly
                value={link}
              />
              <Button
                onClick={() => {
                  void navigator.clipboard.writeText(link).then(() => {
                    setCopied(true);
                  });
                }}
                type="button"
                variant="outline"
              >
                {copied ? <CheckIcon /> : <CopyIcon />}
                {copied ? t("sharing.copied") : t("sharing.copy")}
              </Button>
            </div>
          </Field>
          {rotated ? (
            <p className="text-sm text-muted-foreground" role="status">
              {t("sharing.rotate.done")}
            </p>
          ) : null}
          {readOnly ? null : (
            <AlertDialog>
              <AlertDialogTrigger asChild>
                <Button
                  className="self-start"
                  disabled={update.isPending}
                  variant="outline"
                >
                  {t("sharing.rotate.action")}
                </Button>
              </AlertDialogTrigger>
              <AlertDialogContent>
                <AlertDialogHeader>
                  <AlertDialogTitle>
                    {t("sharing.rotate.title")}
                  </AlertDialogTitle>
                  <AlertDialogDescription>
                    {t("sharing.rotate.description")}
                  </AlertDialogDescription>
                </AlertDialogHeader>
                <AlertDialogFooter>
                  <AlertDialogCancel>
                    {t("dataSources.cancel")}
                  </AlertDialogCancel>
                  <AlertDialogAction
                    onClick={() => {
                      setCopied(false);
                      update.mutate(
                        { linkEnabled: true, rotateToken: true },
                        {
                          onSuccess: () => {
                            setRotated(true);
                          },
                        },
                      );
                    }}
                  >
                    {t("sharing.rotate.confirm")}
                  </AlertDialogAction>
                </AlertDialogFooter>
              </AlertDialogContent>
            </AlertDialog>
          )}
        </div>
      ) : null}
      {update.isError ? (
        <p className="text-sm text-destructive" role="alert">
          {t("sharing.errors.save")}
        </p>
      ) : null}
    </div>
  );
}
