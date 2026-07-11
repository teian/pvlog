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

/** One PV string nested beneath an inverter aggregate. */
export const pvStringSchema = z.object({
  id: z.uuid(),
  inverterId: z.uuid(),
  name: z.string(),
  panelCount: z.number().int().positive(),
  panelManufacturer: z.string().nullable(),
  panelModel: z.string().nullable(),
  ratedPowerWatts: z.number().positive(),
  orientationDegrees: z.number().int().min(0).max(359).nullable(),
  tiltDegrees: z.number().int().min(0).max(90).nullable(),
  effectiveFrom: z.number(),
  effectiveTo: z.number().nullable(),
});

/** Versioned inverter aggregate returned by the modern API. */
export const inverterSchema = z.object({
  id: z.uuid(),
  systemId: z.uuid(),
  name: z.string(),
  manufacturer: z.string().nullable(),
  model: z.string().nullable(),
  serialReference: z.string().nullable(),
  ratedPowerWatts: z.number().nullable(),
  effectiveFrom: z.number(),
  effectiveTo: z.number().nullable(),
  version: z.number().int().positive(),
  strings: z.array(pvStringSchema),
});

/** Generic account/system resource exposed by the existing management endpoints. */
export const managedResourceSchema = z.object({
  id: z.uuid(),
  accountId: z.uuid(),
  systemId: z.uuid().nullable(),
  kind: z.string(),
  version: z.number().int().positive(),
  attributes: z.record(z.string(), z.unknown()),
});

export type LinkedIdentity = z.infer<typeof linkedIdentitySchema>;
export type Role = z.infer<typeof roleSchema>;
export type AuditEvent = z.infer<typeof auditEventSchema>;
export type Invitation = z.infer<typeof invitationSchema>;
export type ConnectorAdmin = z.infer<typeof connectorAdminSchema>;
export type RoleAssignment = z.infer<typeof roleAssignmentSchema>;
export type Inverter = z.infer<typeof inverterSchema>;
export type ManagedResource = z.infer<typeof managedResourceSchema>;
