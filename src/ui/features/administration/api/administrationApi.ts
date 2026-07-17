import {
  auditEventSchema,
  connectorAdminSchema,
  inverterSchema,
  invitationSchema,
  linkedIdentitySchema,
  roleSchema,
  roleAssignmentSchema,
  managedResourceSchema,
  alertRuleSchema,
  webhookSubscriptionSchema,
  administrationUserSchema,
  weatherFeedSettingsSchema,
  emailNotificationSettingsSchema,
  retentionBackupSettingsSchema,
  type AuditEvent,
  type LinkedIdentity,
  type Role,
  type RoleAssignment,
  type Invitation,
  type ConnectorAdmin,
  type Inverter,
  type ManagedResource,
  type AlertRule,
  type WebhookSubscription,
  type AdministrationUser,
  type WeatherFeedSettings,
  type EmailNotificationSettings,
  type RetentionBackupSettings,
} from "@/features/administration/types/administration.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";
import { z } from "zod";

async function getJson(path: string): Promise<unknown> {
  const response = await fetch(path, { credentials: "same-origin" });
  if (!response.ok)
    throw new Error(`request_failed:${String(response.status)}`);
  return response.json();
}

/** Lists connector identities belonging to the signed-in browser user. @returns Validated identity links. */
export async function fetchLinkedIdentities(): Promise<LinkedIdentity[]> {
  return z
    .array(linkedIdentitySchema)
    .parse(await getJson("/api/v1/users/me/identities"));
}

/** Lists roles that can be administered at the current account or instance scope. @param accountId - Account whose role catalog is requested, or null for instance roles. @returns Validated roles. */
export async function fetchRoles(accountId: string | null): Promise<Role[]> {
  return z
    .array(roleSchema)
    .parse(
      await getJson(
        accountId
          ? `/api/v1/accounts/${accountId}/roles`
          : "/api/v1/admin/roles",
      ),
    );
}

/** Lists a bounded, server-authorized audit trail for one account. @param accountId - Account whose audit trail is requested. @returns Validated audit events. */
export async function fetchAuditEvents(
  accountId: string,
): Promise<AuditEvent[]> {
  return z
    .array(auditEventSchema)
    .parse(
      await getJson(`/api/v1/accounts/${accountId}/audit-events?limit=20`),
    );
}

/** Creates a constrained custom role in an account. @param accountId - Account that owns the role. @param input - Role name and selected permissions. @returns The validated created role. */
export async function createRole(
  accountId: string,
  input: { name: string; permissions: string[] },
): Promise<Role> {
  return roleSchema.parse(
    await sessionJsonRequest(`/api/v1/accounts/${accountId}/roles`, {
      method: "POST",
      body: JSON.stringify(input),
    }),
  );
}

/** Assigns a role to a user or API credential at account, system, or instance scope. @param accountId - Account that owns the assignment, or null for an instance assignment. @param input - Validated principal and scope identifiers. @returns The created assignment. */
export async function assignRole(
  accountId: string | null,
  input: {
    roleId: string;
    principalType: "user" | "api_credential";
    principalId: string;
    systemId?: string;
  },
): Promise<RoleAssignment> {
  return roleAssignmentSchema.parse(
    await sessionJsonRequest(
      accountId
        ? `/api/v1/accounts/${accountId}/role-assignments`
        : "/api/v1/admin/role-assignments",
      {
        method: "POST",
        body: JSON.stringify(input),
      },
    ),
  );
}

/** Lists active RBAC assignments for one user. @param accountId - Owning account, or null for instance assignments. @param userId - User principal identifier. @returns Current assignments at the selected scope. */
export async function fetchUserRoleAssignments(
  accountId: string | null,
  userId: string,
): Promise<RoleAssignment[]> {
  const query = new URLSearchParams({
    principalType: "user",
    principalId: userId,
  });
  return z
    .array(roleAssignmentSchema)
    .parse(
      await getJson(
        accountId
          ? `/api/v1/accounts/${accountId}/role-assignments?${query.toString()}`
          : `/api/v1/admin/role-assignments?${query.toString()}`,
      ),
    );
}

/** Creates a one-time local-user invitation. @param email - Email address to invite. @returns The invitation token, shown once to the administrator. */
export async function inviteUser(email: string): Promise<Invitation> {
  return invitationSchema.parse(
    await sessionJsonRequest("/api/v1/admin/user-invitations", {
      method: "POST",
      body: JSON.stringify({ email }),
    }),
  );
}

/** Lists configured connector metadata without secrets. @returns The validated connector catalog. */
export async function fetchConnectors(): Promise<ConnectorAdmin[]> {
  return z
    .array(connectorAdminSchema)
    .parse(await getJson("/api/v1/admin/auth-connectors"));
}

/** Loads one system's complete inverter/string hierarchy. @param systemId - System aggregate root. @returns Validated inverter aggregates. */
export async function fetchInverters(systemId: string): Promise<Inverter[]> {
  return z
    .array(inverterSchema)
    .parse(await getJson(`/api/v1/systems/${systemId}/inverters`));
}

/** Loads generic administration resources. @param path - Authorized modern resource path. @returns Validated resources. */
export async function fetchManagedResources(
  path: string,
): Promise<ManagedResource[]> {
  return z.array(managedResourceSchema).parse(await getJson(path));
}

/** Loads operational administration surface availability without retaining sensitive payloads. @param accountId - Active account. @returns Availability counts for each operational category. */
export async function fetchOperationalSummary(
  accountId: string,
): Promise<Record<string, number | null>> {
  const paths = {
    alerts: `/api/v1/accounts/${accountId}/alerts`,
    alertEvents: `/api/v1/accounts/${accountId}/alert-events`,
    webhooks: `/api/v1/accounts/${accountId}/webhooks`,
    readiness: "/api/v1/health/ready",
  };
  const entries = await Promise.all(
    Object.entries(paths).map(async ([kind, path]) => {
      try {
        const value = await getJson(path);
        return [kind, Array.isArray(value) ? value.length : 1] as const;
      } catch {
        return [kind, null] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}

/** Lists alert rules configured for one account. */
export async function fetchAlertRules(accountId: string): Promise<AlertRule[]> {
  return z
    .array(alertRuleSchema)
    .parse(await getJson(`/api/v1/accounts/${accountId}/alerts`));
}

/** Persists the enabled state of an existing alert rule. */
export async function updateAlertRule(
  accountId: string,
  rule: AlertRule,
): Promise<AlertRule> {
  return alertRuleSchema.parse(
    await sessionJsonRequest(
      `/api/v1/accounts/${accountId}/alerts/${rule.id}`,
      {
        method: "PATCH",
        body: JSON.stringify({
          name: rule.name,
          kind: rule.kind,
          timezone: rule.timezone,
          enabled: rule.enabled,
          condition: rule.condition,
        }),
      },
    ),
  );
}

/** Lists configured webhook notification channels for one account. */
export async function fetchWebhooks(
  accountId: string,
): Promise<WebhookSubscription[]> {
  return z
    .array(webhookSubscriptionSchema)
    .parse(await getJson(`/api/v1/accounts/${accountId}/webhooks`));
}

export async function fetchAdministrationUsers(): Promise<
  AdministrationUser[]
> {
  return z
    .array(administrationUserSchema)
    .parse(await getJson("/api/v1/admin/users"));
}

/** Deletes one eligible local user through the protected lifecycle endpoint. @param userId - Local user identifier. @returns Completion after the server accepts the deletion. */
export async function deleteAdministrationUser(userId: string): Promise<void> {
  await sessionJsonRequest(`/api/v1/admin/users/${userId}`, {
    method: "DELETE",
  });
}

export async function fetchWeatherFeedSettings(): Promise<WeatherFeedSettings> {
  return weatherFeedSettingsSchema.parse(
    await getJson("/api/v1/admin/weather-feed"),
  );
}

export async function saveWeatherFeedSettings(
  settings: WeatherFeedSettings,
): Promise<WeatherFeedSettings> {
  return weatherFeedSettingsSchema.parse(
    await sessionJsonRequest("/api/v1/admin/weather-feed", {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  );
}

export async function fetchEmailNotificationSettings(): Promise<EmailNotificationSettings> {
  return emailNotificationSettingsSchema.parse(
    await getJson("/api/v1/admin/email-notifications"),
  );
}

export async function saveEmailNotificationSettings(
  settings: EmailNotificationSettings,
): Promise<EmailNotificationSettings> {
  return emailNotificationSettingsSchema.parse(
    await sessionJsonRequest("/api/v1/admin/email-notifications", {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  );
}

export async function fetchRetentionBackupSettings(): Promise<RetentionBackupSettings> {
  return retentionBackupSettingsSchema.parse(
    await getJson("/api/v1/admin/retention-backup"),
  );
}

export async function saveRetentionBackupSettings(
  settings: RetentionBackupSettings,
): Promise<RetentionBackupSettings> {
  return retentionBackupSettingsSchema.parse(
    await sessionJsonRequest("/api/v1/admin/retention-backup", {
      method: "PUT",
      body: JSON.stringify(settings),
    }),
  );
}

export async function runBackup(): Promise<unknown> {
  return sessionJsonRequest("/api/v1/admin/backups", { method: "POST" });
}
