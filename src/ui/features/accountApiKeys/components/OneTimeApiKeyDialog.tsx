import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Input,
  Label,
} from "@/shared/components";
import { CheckIcon, CopyIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Presents a newly issued secret once and clears it when dismissed. @param props - Ephemeral secret and close callback. @returns One-time secret dialog. */
export function OneTimeApiKeyDialog({
  apiKey,
  onClose,
}: {
  apiKey: string | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <Dialog
      onOpenChange={(open) => {
        if (!open) {
          setCopied(false);
          onClose();
        }
      }}
      open={apiKey !== null}
    >
      <DialogContent closeLabel={t("accountApiKeys.secret.close")}>
        <DialogHeader>
          <DialogTitle>{t("accountApiKeys.secret.title")}</DialogTitle>
          <DialogDescription>
            {t("accountApiKeys.secret.description")}
          </DialogDescription>
        </DialogHeader>
        <div className="space-y-2">
          <Label htmlFor="issued-api-key">
            {t("accountApiKeys.secret.label")}
          </Label>
          <Input
            className="font-mono"
            id="issued-api-key"
            readOnly
            value={apiKey ?? ""}
          />
        </div>
        <DialogFooter>
          <Button
            onClick={() => {
              if (apiKey)
                void navigator.clipboard.writeText(apiKey).then(() => {
                  setCopied(true);
                });
            }}
            type="button"
          >
            {copied ? <CheckIcon /> : <CopyIcon />}
            {copied
              ? t("accountApiKeys.secret.copied")
              : t("accountApiKeys.secret.copy")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
