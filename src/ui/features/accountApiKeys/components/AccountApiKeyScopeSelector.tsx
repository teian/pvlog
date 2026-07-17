import {
  accountApiKeyScopeKey,
  type AccountApiKeyScope,
} from "@/features/accountApiKeys/types/accountApiKeys.types";
import { useTranslation } from "react-i18next";

const scopes: AccountApiKeyScope[] = [
  "telemetry:write",
  "telemetry:read",
  "systems:read",
  "systems:write",
];

/** Selects explicit independent API scopes with task-oriented descriptions. @param props - Selection, validation state, and update callback. @returns Accessible scope fieldset. */
export function AccountApiKeyScopeSelector({
  invalid,
  selected,
  onChange,
}: {
  invalid: boolean;
  selected: AccountApiKeyScope[];
  onChange: (scopes: AccountApiKeyScope[]) => void;
}) {
  const { t } = useTranslation();
  return (
    <fieldset aria-describedby={invalid ? "api-key-scopes-error" : undefined}>
      <legend className="text-sm font-medium">
        {t("accountApiKeys.permissions")}
      </legend>
      <div className="mt-2 grid gap-3 sm:grid-cols-2">
        {scopes.map((scope) => {
          const key = accountApiKeyScopeKey(scope);
          const id = `api-key-scope-${key}`;
          return (
            <div
              className="flex items-start gap-3 rounded-md border border-border p-3 text-sm"
              key={scope}
            >
              <input
                aria-label={t(`accountApiKeys.scopes.${key}.label`)}
                checked={selected.includes(scope)}
                className="mt-1"
                id={id}
                onChange={(event) => {
                  onChange(
                    event.target.checked
                      ? [...selected, scope]
                      : selected.filter((value) => value !== scope),
                  );
                }}
                type="checkbox"
                value={scope}
              />
              <span>
                <span className="block font-medium">
                  {t(`accountApiKeys.scopes.${key}.label`)}
                </span>
                <span className="block text-muted-foreground">
                  {t(`accountApiKeys.scopes.${key}.description`)}
                </span>
              </span>
            </div>
          );
        })}
      </div>
      {invalid ? (
        <p className="mt-2 text-sm text-destructive" id="api-key-scopes-error">
          {t("accountApiKeys.permissionRequired")}
        </p>
      ) : null}
    </fieldset>
  );
}
