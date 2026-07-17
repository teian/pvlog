import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const accountId = "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c1";
let fetchMock: ReturnType<typeof vi.fn>;
let sessionAccountId: string | null;

describe("administration page", () => {
  beforeEach(async () => {
    sessionAccountId = accountId;
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
          accountId: sessionAccountId,
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
      if (path === "/api/v1/admin/users")
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
            email: "admin@example.test",
            displayName: "Admin",
            status: "active",
            emailVerifiedAt: 1_720_000_000_000,
            disabledAt: null,
            lockedUntil: null,
            createdAt: 1_720_000_000_000,
            updatedAt: 1_720_000_000_000,
          },
        ]);
      if (path === "/api/v1/admin/roles")
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5d1",
            name: "instance_administrator",
            kind: "built_in:InstanceAdministrator",
            permissions: ["instance_manage", "role_manage"],
            parentRoleIds: [],
            version: 1,
            createdAt: 1_720_000_000_000,
            updatedAt: 1_720_000_000_000,
          },
        ]);
      if (path.includes("/api/v1/admin/role-assignments?principalType=user"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5d2",
            roleId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5d1",
            principalType: "user",
            principalId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
            accountId: null,
            systemId: null,
            expiresAt: null,
          },
        ]);
      if (path === "/api/v1/admin/weather-feed")
        return json({
          enabled: true,
          endpoint: "mqtt://broker.example.test:1883/weather",
          credentialSecretRef: null,
          updatedAtEpochMillis: null,
        });
      if (path === "/api/v1/admin/email-notifications")
        return json({
          enabled: true,
          recipient: "ops@example.test",
          host: "smtp.example.test",
          port: 587,
          username: "alerts@example.test",
          credentialSecretRef: "secret://smtp/pvlog",
          encryption: "starttls",
          updatedAtEpochMillis: null,
        });
      if (path === "/api/v1/admin/retention-backup")
        return json({
          readingRetentionDays: 365,
          automaticBackupsEnabled: true,
          backupSchedule: "0 2 * * *",
          lastBackupAtEpochMillis: null,
          lastBackupBytes: null,
          updatedAtEpochMillis: null,
        });
      if (path === "/api/v1/admin/user-invitations" && init?.method === "POST")
        return new Response(
          JSON.stringify({
            invitationId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c8",
            activationToken: "one-time-token",
            expiresAt: 1_720_000_000_000,
          }),
          { status: 201, headers: { "content-type": "application/json" } },
        );
      if (
        path === "/api/v1/admin/users/018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2" &&
        init?.method === "DELETE"
      )
        return new Response(null, { status: 204 });
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
      if (path.includes("/role-assignments?principalType=user"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c9",
            roleId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
            principalType: "user",
            principalId: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
            accountId,
            systemId: null,
            expiresAt: null,
          },
        ]);
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
      if (path.endsWith("/alerts/018f2ab5-8a75-7cc4-9a9b-b0f10c37d5ca"))
        return json({
          id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5ca",
          name: "Low yield",
          kind: "performance_below",
          timezone: "Europe/Berlin",
          enabled: true,
          condition: { percentage: 70 },
        });
      if (path.endsWith("/alerts"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5ca",
            name: "Low yield",
            kind: "performance_below",
            timezone: "Europe/Berlin",
            enabled: false,
            condition: { percentage: 70 },
          },
        ]);
      if (path.endsWith("/webhooks"))
        return json([
          {
            id: "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5cb",
            endpoint: "https://ops.example.test/pvlog",
            events: ["alert_opened", "alert_resolved"],
            state: "active",
          },
        ]);
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("uses the dedicated administration navigation to separate account data", async () => {
    const user = userEvent.setup();
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
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "Role" })).toHaveValue(
        "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
      );
    });
    expect(screen.queryByText("Company SSO")).not.toBeInTheDocument();
    expect(screen.queryByText("role.create")).not.toBeInTheDocument();

    await user.click(screen.getByRole("link", { name: "Data Sources" }));
    expect(await screen.findByText("Company SSO")).toBeVisible();

    await user.click(screen.getByRole("link", { name: "System Logs" }));
    expect(await screen.findByText("role.create")).toBeVisible();
  });

  it("shows the seeded instance administrator without requiring an account", async () => {
    sessionAccountId = null;
    render(<App runtimeConfig={runtimeConfig} />);

    expect(await screen.findByText("admin@example.test")).toBeVisible();
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "Role" })).toHaveValue(
        "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5d1",
      );
    });
    expect(screen.getByRole("option", { name: "Admin" })).toBeVisible();
  });

  it("toggles a real account alert rule through its PATCH endpoint", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);

    await user.click(await screen.findByRole("link", { name: "Alert Rules" }));
    await screen.findByText("Low yield");
    await user.click(screen.getByRole("switch", { name: "Low yield" }));

    expect(fetchMock).toHaveBeenCalledWith(
      `/api/v1/accounts/${accountId}/alerts/018f2ab5-8a75-7cc4-9a9b-b0f10c37d5ca`,
      expect.objectContaining({
        method: "PATCH",
        body: JSON.stringify({
          name: "Low yield",
          kind: "performance_below",
          timezone: "Europe/Berlin",
          enabled: true,
          condition: { percentage: 70 },
        }),
      }),
    );
  });

  it("shows configured webhook notification channels", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);

    await user.click(
      await screen.findByRole("link", { name: "Notifications" }),
    );
    expect(
      await screen.findByText("https://ops.example.test/pvlog"),
    ).toBeVisible();
    expect(screen.getByText("2 subscribed events")).toBeVisible();
  });

  it("deletes a user through the protected lifecycle endpoint", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);
    await user.click(
      await screen.findByRole("button", { name: "Delete Admin" }),
    );
    await user.click(screen.getByRole("button", { name: "Delete user" }));

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/v1/admin/users/018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c2",
      expect.objectContaining({
        method: "DELETE",
      }),
    );
  });

  it("shows an invitation token only from the creation response", async () => {
    const user = userEvent.setup();
    render(<App runtimeConfig={runtimeConfig} />);
    await user.click(
      await screen.findByRole("button", { name: "Invite User" }),
    );
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
    await user.selectOptions(
      await screen.findByRole("combobox", { name: "Role" }),
      "018f2ab5-8a75-7cc4-9a9b-b0f10c37d5c5",
    );

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
