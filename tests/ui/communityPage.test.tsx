import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const systemId = "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c1";
const secondSystemId = "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2";
let fetchMock: ReturnType<typeof vi.fn>;

describe("community page", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", "/community");
    window.sessionStorage.setItem("pvlog.csrf-token", "csrf-token");
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = input instanceof Request ? input.url : String(input);
      if (path === "/api/v1/session")
        return json({
          authenticated: true,
          user: { id: systemId, displayName: "Operator" },
          accountId: systemId,
          systemIds: [systemId, secondSystemId],
          permissions: ["analytics:read"],
          connectors: [],
        });
      if (path.startsWith("/api/v1/community/systems"))
        return json([communitySystem("Neighbour", systemId)]);
      if (path === "/api/v1/users/me/favourites") return json([]);
      if (path === "/api/v1/ladders?metric=normalized_generation")
        return json([comparisonEntry("Neighbour", systemId)]);
      if (path === "/api/v1/comparisons" && init?.method === "POST")
        return json([comparisonEntry("Mine", systemId)]);
      if (
        path === `/api/v1/users/me/favourites/${systemId}` &&
        init?.method === "POST"
      )
        return json(communitySystem("Neighbour", systemId));
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("renders discovery and projection freshness information", async () => {
    render(<App runtimeConfig={runtimeConfig} />);

    expect(await screen.findByRole("heading", { name: "Community" })).toBeVisible();
    expect(await screen.findByText("Neighbour")).toBeVisible();
    expect(screen.getByText("Projection data may be out of date.")).toBeVisible();
  });

  it("adds a favourite and compares active-session systems", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);

    await user.click(await screen.findByRole("button", { name: "Add favourite" }));
    expect(fetchMock).toHaveBeenCalledWith(
      `/api/v1/users/me/favourites/${systemId}`,
      expect.objectContaining({ method: "POST" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Compare my systems" }),
    );
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/comparisons",
      expect.objectContaining({
        body: JSON.stringify({
          systemIds: [systemId, secondSystemId],
          metric: "normalized_generation",
        }),
        method: "POST",
      }),
    );
  });
});

function communitySystem(displayName: string, id: string) {
  return {
    systemId: id,
    displayName,
    countryCode: "DE",
    locationLabel: "Berlin",
    locationPrecision: "city",
    capacityWatts: 5_000,
    activity: "active",
    projectionAgeMillis: 3_000,
    projectionLagEvents: 0,
    stale: true,
  };
}

function comparisonEntry(displayName: string, id: string) {
  return {
    rank: 1,
    systemId: id,
    displayName,
    totalGenerationWh: 12_000,
    normalizedGenerationWhPerKw: 2_400,
    coverageBasisPoints: 9_900,
    tied: false,
    projectionAgeMillis: 3_000,
  };
}

function json(value: unknown): Response {
  return new Response(JSON.stringify(value), {
    status: 200,
    headers: { "content-type": "application/json" },
  });
}

const runtimeConfig = {
  apiBaseUrl: "/api",
  telemetry: {
    enabled: false,
    headers: {},
    serviceName: "pvlog-ui",
    serviceVersion: "test",
  },
};
