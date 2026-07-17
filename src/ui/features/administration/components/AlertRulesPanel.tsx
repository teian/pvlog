import {
  useAlertRules,
  useUpdateAlertRule,
} from "@/features/administration/hooks/useAdministration";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
  Switch,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders live account alert rules and allows administrators to enable them. */
export function AlertRulesPanel({
  accountId,
}: {
  accountId: string | null | undefined;
}) {
  const { t } = useTranslation();
  const rules = useAlertRules(accountId);
  const updateRule = useUpdateAlertRule(accountId);

  return (
    <Card className="gap-0 overflow-hidden py-0">
      <CardHeader className="border-b py-5">
        <CardTitle>{t("administration.alertRules.title")}</CardTitle>
        <CardDescription>
          {t("administration.alertRules.description")}
        </CardDescription>
      </CardHeader>
      <CardContent className="px-0">
        {rules.isLoading ? <Skeleton className="m-5 h-28" /> : null}
        {rules.isError ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.restricted")}
          </p>
        ) : null}
        {rules.data?.length === 0 ? (
          <p className="p-5 text-sm text-muted-foreground">
            {t("administration.alertRules.empty")}
          </p>
        ) : null}
        <ul aria-label={t("administration.alertRules.title")}>
          {rules.data?.map((rule) => (
            <li
              className="flex items-center gap-4 border-b px-5 py-4 last:border-b-0"
              key={rule.id}
            >
              <label className="min-w-0 flex-1" htmlFor={`rule-${rule.id}`}>
                <span className="block text-sm font-semibold">{rule.name}</span>
                <span className="block text-sm text-muted-foreground">
                  {t(`administration.alertRules.kinds.${rule.kind}`, {
                    defaultValue: rule.kind,
                  })}
                </span>
              </label>
              <Switch
                aria-label={rule.name}
                checked={rule.enabled}
                disabled={updateRule.isPending}
                id={`rule-${rule.id}`}
                onCheckedChange={(enabled) => {
                  updateRule.mutate({ ...rule, enabled });
                }}
              />
            </li>
          ))}
        </ul>
        {updateRule.isError ? (
          <p
            className="border-t px-5 py-3 text-sm text-destructive"
            role="alert"
          >
            {t("administration.alertRules.updateError")}
          </p>
        ) : null}
      </CardContent>
    </Card>
  );
}
