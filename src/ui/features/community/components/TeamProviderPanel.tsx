import {
  useRegionalSupply,
  useTeamManagement,
} from "@/features/community/hooks/useCommunity";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  FieldLabel,
  Input,
} from "@/shared/components";
import { useState, type SyntheticEvent } from "react";
import { useTranslation } from "react-i18next";

/** Manages team creation/membership and displays regional provider freshness. @param props - Active account and systems. @returns Team and provider workflows. */
export function TeamProviderPanel({
  accountId,
  systemIds,
}: {
  accountId: string | null | undefined;
  systemIds: string[];
}) {
  const { t } = useTranslation();
  const teams = useTeamManagement();
  const providers = useRegionalSupply();
  const [name, setName] = useState("");
  function submit(event: SyntheticEvent<HTMLFormElement>): void {
    event.preventDefault();
    if (!accountId) return;
    teams.create.mutate(
      { accountId, name },
      {
        onSuccess: (team) => {
          setName("");
          if (systemIds[0]) {
            teams.join.mutate({ teamId: team.id, systemId: systemIds[0] });
          }
        },
      },
    );
  }
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("community.teams.title")}</CardTitle>
        <CardDescription>{t("community.teams.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-6">
        <form className="flex flex-wrap items-end gap-3" onSubmit={submit}>
          <div className="min-w-56 flex-1 space-y-2">
            <FieldLabel htmlFor="team-name">
              {t("community.teams.name")}
            </FieldLabel>
            <Input
              id="team-name"
              onChange={(event) => {
                setName(event.target.value);
              }}
              required
              value={name}
            />
          </div>
          <Button
            disabled={!accountId || !systemIds[0] || !name || teams.create.isPending}
            type="submit"
          >
            {t("community.teams.createAndJoin")}
          </Button>
        </form>
        <section aria-labelledby="regional-supply-title" className="space-y-3">
          <h3
            className="text-sm font-semibold uppercase tracking-widest text-muted-foreground"
            id="regional-supply-title"
          >
            {t("community.providers.title")}
          </h3>
          {providers.isError ? (
            <p className="text-sm text-muted-foreground">
              {t("community.providers.unavailable")}
            </p>
          ) : null}
          <ul className="space-y-2">
            {providers.data?.map((provider) => (
              <li className="rounded-md border p-3" key={provider.regionKey}>
                <p className="font-medium">{provider.regionKey}</p>
                <p className="text-sm text-muted-foreground">
                  {t("community.providers.metadata", {
                    source: provider.source,
                    license: provider.license,
                    state: provider.stale
                      ? t("community.providers.stale")
                      : t("community.providers.fresh"),
                  })}
                </p>
              </li>
            ))}
          </ul>
        </section>
      </CardContent>
    </Card>
  );
}
