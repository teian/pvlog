import { z } from "zod";

/** An external identity linked to the signed-in browser user. */
export const linkedIdentitySchema = z.object({
  id: z.uuid(),
  connectorId: z.uuid(),
  subject: z.string(),
  linkedAtEpochMillis: z.number(),
  lastLoginAtEpochMillis: z.number().nullable(),
});

/** An account role visible to a role administrator. */
export const roleSchema = z.object({
  id: z.uuid(),
  name: z.string(),
  kind: z.string(),
  permissions: z.array(z.string()),
  parentRoleIds: z.array(z.uuid()),
  version: z.number(),
  createdAt: z.number(),
  updatedAt: z.number(),
});

/** An audit event with server-filtered, safe metadata. */
export const auditEventSchema = z.object({
  id: z.uuid(),
  occurredAt: z.number(),
  actorType: z.string(),
  actorId: z.uuid().nullable(),
  action: z.string(),
  targetType: z.string(),
  targetId: z.uuid().nullable(),
  outcome: z.string(),
  safeMetadata: z.unknown(),
});

/** One-time invitation material returned only to the authorized administrator. */
export const invitationSchema = z.object({
  invitationId: z.uuid(),
  activationToken: z.string().min(1),
  expiresAt: z.number(),
});

/** Non-secret connector metadata available to authorized instance administrators. */
export const connectorAdminSchema = z.object({
  id: z.string().min(1),
  displayName: z.string().min(1),
  protocol: z.enum(["oidc", "oauth2"]),
  enabled: z.boolean(),
  authorizationEndpoint: z.url().nullable(),
  scopes: z.array(z.string()),
});

/** A persisted account- or system-scoped role assignment. */
export const roleAssignmentSchema = z.object({
  id: z.uuid(),
  roleId: z.uuid(),
  principalType: z.enum(["user", "api_credential"]),
  principalId: z.uuid(),
  accountId: z.uuid(),
  systemId: z.uuid().nullable(),
  expiresAt: z.number().nullable(),
});

export type LinkedIdentity = z.infer<typeof linkedIdentitySchema>;
export type Role = z.infer<typeof roleSchema>;
export type AuditEvent = z.infer<typeof auditEventSchema>;
export type Invitation = z.infer<typeof invitationSchema>;
export type ConnectorAdmin = z.infer<typeof connectorAdminSchema>;
export type RoleAssignment = z.infer<typeof roleAssignmentSchema>;
