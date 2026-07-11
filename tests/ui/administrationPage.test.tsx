import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const accountId = "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c1";
let fetchMock: ReturnType<typeof vi.fn>;

describe("administration page", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", "/administration");
    window.sessionStorage.setItem("pvlog.csrf-token", "csrf-token");
    fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const path = input instanceof Request ? input.url : String(input);
      if (path === "/api/v1/session")
        return json({
          authenticated: true,
          user: {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
            displayName: "Admin",
          },
          accountId,
          systemIds: [],
          permissions: [],
          connectors: [],
        });
      if (path === "/api/v1/users/me/identities")
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c3",
            connectorId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c4",
            subject: "admin@example.test",
            linkedAtEpochMillis: 1_720_000_000_000,
            lastLoginAtEpochMillis: null,
          },
        ]);
      if (path === "/api/v1/admin/auth-connectors")
        return json([
          {
            id: "company-sso",
            displayName: "Company SSO",
            protocol: "oidc",
            enabled: true,
            authorizationEndpoint: "https://identity.example/authorize",
            scopes: ["openid"],
          },
        ]);
      if (path === "/api/v1/admin/user-invitations" && init?.method === "POST")
        return new Response(
          JSON.stringify({
            invitationId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c8",
            activationToken: "one-time-token",
            expiresAt: 1_720_000_000_000,
          }),
          { status: 201, headers: { "content-type": "application/json" } },
        );
      if (path.endsWith("/role-assignments") && init?.method === "POST")
        return json({
          id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c9",
          roleId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
          principalType: "user",
          principalId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
          accountId,
          systemId: null,
          expiresAt: null,
        });
      if (path.endsWith("/roles") && init?.method === "POST")
        return json({
          id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c7",
          name: "Readers",
          kind: "custom",
          permissions: ["system_read"],
          parentRoleIds: [],
          version: 1,
          createdAt: 1_720_000_000_000,
          updatedAt: 1_720_000_000_000,
        });
      if (path.endsWith("/roles"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
            name: "Operators",
            kind: "custom",
            permissions: ["systems:write"],
            parentRoleIds: [],
            version: 1,
            createdAt: 1_720_000_000_000,
            updatedAt: 1_720_000_000_000,
          },
        ]);
      if (path.includes("/audit-events?limit=20"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c6",
            occurredAt: 1_720_000_000_000,
            actorType: "user",
            actorId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
            action: "role.create",
            targetType: "role",
            targetId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
            outcome: "success",
            safeMetadata: {},
          },
        ]);
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("shows only the authorized identity, role, and audit data", async () => {
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: {
            enabled: false,
            headers: {},
            serviceName: "pvlog-ui",
            serviceVersion: "test",
          },
        }}
      />,
    );

    expect(
      await screen.findByRole("heading", { name: "Administration" }),
    ).toBeVisible();
    expect(await screen.findByText("admin@example.test")).toBeVisible();
    expect(screen.getAllByText("Operators")).not.toHaveLength(0);
    expect(screen.getByText("Company SSO")).toBeVisible();
    expect(screen.getByText("role.create")).toBeVisible();
  });

  it("creates a role through the CSRF-protected account endpoint", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);
    await screen.findByRole("heading", { name: "Administration" });
    await user.type(await screen.findByLabelText("Role name"), "Readers");
    await user.click(screen.getByLabelText("Read systems"));
    await user.click(screen.getByRole("button", { name: "Create role" }));

    expect(fetchMock).toHaveBeenCalledWith(
      `/api/v1/accounts/${accountId}/roles`,
      expect.objectContaining({
        body: JSON.stringify({ name: "Readers", permissions: ["system_read"] }),
        method: "POST",
      }),
    );
  });

  it("shows an invitation token only from the creation response", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);
    await user.type(
      await screen.findByLabelText("Email address"),
      "invitee@example.test",
    );
    await user.click(screen.getByRole("button", { name: "Create invitation" }));

    expect(await screen.findByText("one-time-token")).toBeVisible();
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/admin/user-invitations",
      expect.objectContaining({
        body: JSON.stringify({ email: "invitee@example.test" }),
        method: "POST",
      }),
    );
  });

  it("assigns an existing role to a typed principal", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);
    await user.type(
      await screen.findByLabelText("Principal ID"),
      "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
    );
    await user.click(screen.getByRole("button", { name: "Assign role" }));

    expect(fetchMock).toHaveBeenCalledWith(
      `/api/v1/accounts/${accountId}/role-assignments`,
      expect.objectContaining({
        body: JSON.stringify({
          roleId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
          principalType: "user",
          principalId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
        }),
        method: "POST",
      }),
    );
  });
});

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
