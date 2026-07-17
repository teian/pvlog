import { AccountApiKeyScopeSelector } from "@/features/accountApiKeys/components/AccountApiKeyScopeSelector";
import type {
  AccountApiKeyScope,
  CreateAccountApiKeyInput,
} from "@/features/accountApiKeys/types/accountApiKeys.types";
import { Button, Input, Label } from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Accessible least-privilege account API-key creation form. @param props - Submit state and validated submit callback. @returns Account API-key form. */
export function AccountApiKeyCreateForm({
  pending,
  onSubmit,
}: {
  pending: boolean;
  onSubmit: (input: CreateAccountApiKeyInput) => void;
}) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [selected, setSelected] = useState<AccountApiKeyScope[]>([]);
  const [expiry, setExpiry] = useState("");
  const [attempted, setAttempted] = useState(false);
  const invalid = name.trim() === "" || selected.length === 0;
  function submit(event: SyntheticEvent<HTMLFormElement>) {
    event.preventDefault();
    setAttempted(true);
    if (invalid) return;
    onSubmit({
      name: name.trim(),
      scopes: selected,
      expiresAtEpochMillis: expiry ? new Date(expiry).getTime() : null,
    });
  }
  return (
    <form className="space-y-5" onSubmit={submit}>
      <div className="space-y-2">
        <Label htmlFor="api-key-name">{t("accountApiKeys.name")}</Label>
        <Input
          aria-describedby={
            attempted && !name.trim() ? "api-key-name-error" : undefined
          }
          aria-invalid={attempted && !name.trim()}
          id="api-key-name"
          onChange={(event) => {
            setName(event.target.value);
          }}
          value={name}
        />
        {attempted && !name.trim() ? (
          <p className="text-sm text-destructive" id="api-key-name-error">
            {t("accountApiKeys.nameRequired")}
          </p>
        ) : null}
      </div>
      <AccountApiKeyScopeSelector
        invalid={attempted && selected.length === 0}
        onChange={setSelected}
        selected={selected}
      />
      <div className="space-y-2">
        <Label htmlFor="api-key-expiry">{t("accountApiKeys.expiry")}</Label>
        <Input
          id="api-key-expiry"
          min={new Date().toISOString().slice(0, 16)}
          onChange={(event) => {
            setExpiry(event.target.value);
          }}
          type="datetime-local"
          value={expiry}
        />
        <p className="text-xs text-muted-foreground">
          {t("accountApiKeys.expiryHint")}
        </p>
      </div>
      <Button disabled={pending} type="submit">
        {pending ? t("accountApiKeys.creating") : t("accountApiKeys.create")}
      </Button>
    </form>
  );
}
