import { readFileSync } from "node:fs";

const spec = readFileSync("openapi/pvlog-v1.yaml", "utf8");
const required = [
  ["get", "/api/v1/health/version", "getBuildVersion"],
  ["get", "/api/v1/health/ready", "getReadiness"],
  ["post", "/api/v1/systems", "createSystem"],
  ["put", "/api/v1/systems/{id}", "updateSystem"],
  ["delete", "/api/v1/systems/{id}", "deleteSystem"],
  ["post", "/api/v1/systems/{id}/archive", "archiveSystem"],
  ["post", "/api/v1/systems/{id}/restore", "restoreSystem"],
  [
    "get",
    "/api/v1/accounts/{account_id}/systems/{system_id}/equipment",
    "listEquipment",
  ],
  [
    "post",
    "/api/v1/accounts/{account_id}/systems/{system_id}/equipment",
    "createEquipment",
  ],
  [
    "get",
    "/api/v1/accounts/{account_id}/systems/{system_id}/tariffs",
    "listTariffs",
  ],
  [
    "post",
    "/api/v1/accounts/{account_id}/systems/{system_id}/tariffs",
    "createTariff",
  ],
  [
    "get",
    "/api/v1/accounts/{account_id}/systems/{system_id}/channels",
    "listChannels",
  ],
  [
    "post",
    "/api/v1/accounts/{account_id}/systems/{system_id}/channels",
    "createChannel",
  ],
  ["get", "/api/v1/accounts/{account_id}/memberships", "listMemberships"],
  ["post", "/api/v1/accounts/{account_id}/memberships", "createMembership"],
  ["get", "/api/v1/accounts/{account_id}/credentials", "listCredentials"],
  ["post", "/api/v1/accounts/{account_id}/credentials", "createCredential"],
  ["post", "/api/v1/systems/{system_id}/observations", "createObservation"],
  [
    "post",
    "/api/v1/systems/{system_id}/observations/batch",
    "createObservationBatch",
  ],
  [
    "patch",
    "/api/v1/systems/{system_id}/observations/{observation_id}",
    "correctObservation",
  ],
  [
    "delete",
    "/api/v1/systems/{system_id}/observations/{observation_id}",
    "deleteObservation",
  ],
  ["get", "/api/v1/systems/{system_id}/series", "getSystemSeries"],
  ["get", "/api/v1/systems/{system_id}/statistics", "getSystemStatistics"],
  ["get", "/api/v1/systems/{system_id}/data-quality", "getSystemDataQuality"],
  ["post", "/api/v1/comparisons", "compareSystems"],
  ["get", "/api/v1/ladders", "getLadder"],
  [
    "post",
    "/api/v1/systems/{system_id}/analysis-exports",
    "createAnalysisExport",
  ],
  ["get", "/api/v1/session", "getBrowserSession"],
  ["post", "/api/v1/session", "logoutBrowserSession"],
  ["post", "/api/v1/auth/local/login", "loginWithLocalPassword"],
  ["post", "/api/v1/auth/invitations/accept", "acceptInvitation"],
  ["get", "/api/v1/admin/auth-connectors", "listAuthConnectors"],
  ["post", "/api/v1/admin/user-invitations", "createLocalUserInvitation"],
  ["get", "/api/v1/users/me/identities", "listLinkedIdentities"],
  [
    "get",
    "/api/v1/accounts/{account_id}/audit-events",
    "listAccountAuditEvents",
  ],
  ["get", "/api/v1/accounts/{account_id}/roles", "listAccountRoles"],
  ["post", "/api/v1/accounts/{account_id}/roles", "createAccountRole"],
  [
    "patch",
    "/api/v1/accounts/{account_id}/roles/{role_id}",
    "updateAccountRole",
  ],
  [
    "delete",
    "/api/v1/accounts/{account_id}/roles/{role_id}",
    "deleteAccountRole",
  ],
  [
    "post",
    "/api/v1/accounts/{account_id}/role-assignments",
    "assignAccountRole",
  ],
  [
    "delete",
    "/api/v1/accounts/{account_id}/role-assignments/{assignment_id}",
    "revokeAccountRoleAssignment",
  ],
];
for (const [method, path, operationId] of required) {
  const pathAt = spec.indexOf(`  ${path}:`);
  const nextPath = spec.indexOf("\n  /api/", pathAt + 1);
  const block = spec.slice(
    pathAt,
    nextPath < 0 ? spec.indexOf("\ncomponents:") : nextPath,
  );
  if (
    pathAt < 0 ||
    !block.includes(`    ${method}:`) ||
    !block.includes(`operationId: ${operationId}`)
  )
    throw new Error(`missing ${method.toUpperCase()} ${path} (${operationId})`);
}
console.log(`OpenAPI route coverage: ${required.length} operations`);
