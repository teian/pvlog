import { Button } from "@/shared/components";
import { CheckIcon, CopyIcon } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Copies a value to the clipboard and confirms it in place. @param props - Value to copy and the accessible name of what is copied. @returns An icon button that announces the copy. */
export function CopyButton({ value, label }: { value: string; label: string }) {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  return (
    <Button
      aria-label={
        copied
          ? t("dataSources.configuration.copied", { label })
          : t("dataSources.configuration.copy", { label })
      }
      onClick={() => {
        void navigator.clipboard.writeText(value).then(() => {
          setCopied(true);
        });
      }}
      size="sm"
      type="button"
      variant="outline"
    >
      {copied ? <CheckIcon /> : <CopyIcon />}
    </Button>
  );
}
