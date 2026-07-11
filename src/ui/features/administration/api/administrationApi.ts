import {
  auditEventSchema,
  connectorAdminSchema,
  inverterSchema,
  invitationSchema,
  linkedIdentitySchema,
  roleSchema,
  roleAssignmentSchema,
  managedResourceSchema,
  type AuditEvent,
  type LinkedIdentity,
  type Role,
  type RoleAssignment,
  type Invitation,
  type ConnectorAdmin,
  type Inverter,
  type ManagedResource,
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

/** Lists roles that can be administered for one account. @param accountId - Account whose role catalog is requested. @returns Validated roles. */
export async function fetchRoles(accountId: string): Promise<Role[]> {
  return z
    .array(roleSchema)
    .parse(await getJson(`/api/v1/accounts/${accountId}/roles`));
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

/** Assigns a role to a user or API credential at account or system scope. @param accountId - Account that owns the assignment. @param input - Validated principal and scope identifiers. @returns The created assignment. */
export async function assignRole(
  accountId: string,
  input: {
    roleId: string;
    principalType: "user" | "api_credential";
    principalId: string;
    systemId?: string;
  },
): Promise<RoleAssignment> {
  return roleAssignmentSchema.parse(
    await sessionJsonRequest(`/api/v1/accounts/${accountId}/role-assignments`, {
      method: "POST",
      body: JSON.stringify(input),
    }),
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

/** Loads one system's complete inverter/string hierarchy. @param accountId - Owning account. @param systemId - System aggregate root. @returns Validated inverter aggregates. */
export async function fetchInverters(
  accountId: string,
  systemId: string,
): Promise<Inverter[]> {
  return z
    .array(inverterSchema)
    .parse(
      await getJson(
        `/api/v1/accounts/${accountId}/systems/${systemId}/inverters`,
      ),
    );
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
