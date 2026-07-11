import {
  useCommunitySearch,
  useFavouriteMutation,
} from "@/features/community/hooks/useCommunity";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Input,
  Label,
  Skeleton,
} from "@/shared/components";
import { useState } from "react";
import { useTranslation } from "react-i18next";

/** Renders searchable, privacy-safe community system projections. @returns The community discovery panel. */
export function CommunitySearchPanel() {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [countryCode, setCountryCode] = useState("");
  const search = useCommunitySearch({ query, countryCode });
  const favourite = useFavouriteMutation();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("community.search.title")}</CardTitle>
        <CardDescription>{t("community.search.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="space-y-2">
            <Label htmlFor="community-query">
              {t("community.search.query")}
            </Label>
            <Input
              id="community-query"
              onChange={(event) => {
                setQuery(event.target.value);
              }}
              placeholder={t("community.search.queryPlaceholder")}
              value={query}
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="community-country">
              {t("community.search.country")}
            </Label>
            <Input
              id="community-country"
              maxLength={2}
              onChange={(event) => {
                setCountryCode(event.target.value.toUpperCase());
              }}
              placeholder={t("community.search.countryPlaceholder")}
              value={countryCode}
            />
          </div>
        </div>
        {search.isLoading ? <Skeleton className="h-24 w-full" /> : null}
        {search.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("community.unavailable")}
          </p>
        ) : null}
        {search.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("community.search.empty")}
          </p>
        ) : null}
        <ul aria-label={t("community.search.title")} className="space-y-3">
          {search.data?.map((system) => (
            <li
              className="flex flex-wrap items-center justify-between gap-3 rounded-md border p-3"
              key={system.systemId}
            >
              <div>
                <p className="font-medium">{system.displayName}</p>
                <p className="text-sm text-muted-foreground">
                  {t("community.search.details", {
                    capacity: system.capacityWatts,
                    country: system.countryCode ?? t("community.search.unknown"),
                    location:
                      system.locationLabel ?? t("community.search.unknown"),
                  })}
                </p>
                {system.stale ? (
                  <p className="text-sm text-muted-foreground">
                    {t("community.search.stale")}
                  </p>
                ) : null}
              </div>
              <Button
                disabled={favourite.isPending}
                onClick={() => {
                  favourite.mutate({ systemId: system.systemId, favourite: true });
                }}
                type="button"
                variant="secondary"
              >
                {t("community.favourites.add")}
              </Button>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
