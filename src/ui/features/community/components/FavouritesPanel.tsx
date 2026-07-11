import {
  useFavouriteMutation,
  useFavourites,
} from "@/features/community/hooks/useCommunity";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Skeleton,
} from "@/shared/components";
import { useTranslation } from "react-i18next";

/** Renders and manages the active user's visible-system favourites. @returns The favourites panel. */
export function FavouritesPanel() {
  const { t } = useTranslation();
  const favourites = useFavourites();
  const favourite = useFavouriteMutation();
  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("community.favourites.title")}</CardTitle>
        <CardDescription>
          {t("community.favourites.description")}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {favourites.isLoading ? <Skeleton className="h-16 w-full" /> : null}
        {favourites.isError ? (
          <p className="text-sm text-muted-foreground">
            {t("community.unavailable")}
          </p>
        ) : null}
        {favourites.data?.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {t("community.favourites.empty")}
          </p>
        ) : null}
        <ul aria-label={t("community.favourites.title")} className="space-y-3">
          {favourites.data?.map((system) => (
            <li
              className="flex items-center justify-between gap-3 rounded-md border p-3"
              key={system.systemId}
            >
              <div>
                <p className="font-medium">{system.displayName}</p>
                <p className="text-sm text-muted-foreground">
                  {t("community.favourites.capacity", {
                    capacity: system.capacityWatts,
                  })}
                </p>
              </div>
              <Button
                disabled={favourite.isPending}
                onClick={() => {
                  favourite.mutate({ systemId: system.systemId, favourite: false });
                }}
                type="button"
                variant="outline"
              >
                {t("community.favourites.remove")}
              </Button>
            </li>
          ))}
        </ul>
      </CardContent>
    </Card>
  );
}
