import {
  communitySystemSchema,
  comparisonEntrySchema,
  type ComparisonEntry,
  type CommunitySearchFilters,
  type CommunitySystem,
} from "@/features/community/types/community.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

async function getJson(path: string): Promise<unknown> {
  const response = await fetch(path, { credentials: "same-origin" });
  if (!response.ok)
    throw new Error(`request_failed:${String(response.status)}`);
  return response.json();
}

/** Searches privacy-safe public system projections. @param filters - Optional name and country filters. @returns Validated community systems. */
export async function fetchCommunitySystems(
  filters: CommunitySearchFilters,
): Promise<CommunitySystem[]> {
  const parameters = new URLSearchParams();
  if (filters.query) parameters.set("query", filters.query);
  if (filters.countryCode) parameters.set("countryCode", filters.countryCode);
  const suffix = parameters.size === 0 ? "" : `?${parameters.toString()}`;
  return z
    .array(communitySystemSchema)
    .parse(await getJson(`/api/v1/community/systems${suffix}`));
}

/** Lists community systems favourited by the active browser user. @returns Validated favourites. */
export async function fetchFavourites(): Promise<CommunitySystem[]> {
  return z
    .array(communitySystemSchema)
    .parse(await getJson("/api/v1/users/me/favourites"));
}

/** Adds a visible community system to the active user's favourites. @param systemId - System to favourite. @returns The validated favourite. */
export async function addFavourite(systemId: string): Promise<CommunitySystem> {
  return communitySystemSchema.parse(
    await sessionJsonRequest(`/api/v1/users/me/favourites/${systemId}`, {
      method: "POST",
    }),
  );
}

/** Removes a community system from the active user's favourites. @param systemId - System to remove. @returns Completion once the server accepts the removal. */
export async function removeFavourite(systemId: string): Promise<void> {
  await sessionJsonRequest(`/api/v1/users/me/favourites/${systemId}`, {
    method: "DELETE",
  });
}

/** Loads a public normalized-generation ladder. @returns Validated ladder rows. */
export async function fetchLadder(): Promise<ComparisonEntry[]> {
  return z
    .array(comparisonEntrySchema)
    .parse(await getJson("/api/v1/ladders?metric=normalized_generation"));
}

/** Compares two or more systems visible to the active user. @param systemIds - Between two and twenty system identifiers. @returns Validated comparison rows. */
export async function compareSystems(
  systemIds: string[],
): Promise<ComparisonEntry[]> {
  return z
    .array(comparisonEntrySchema)
    .parse(
      await sessionJsonRequest("/api/v1/comparisons", {
        method: "POST",
        body: JSON.stringify({
          systemIds,
          metric: "normalized_generation",
        }),
      }),
    );
}
