import { readFileSync, readdirSync } from "node:fs";

const spec = readFileSync("openapi/pvlog-v1.yaml", "utf8");
const required = [
  ["get", "/api/v1/health/version", "getBuildVersion"],
  ["get", "/api/v1/health/ready", "getReadiness"],
  ["get", "/api/v1/health/dependencies", "getDependencyHealth"],
  ["get", "/api/v1/health/job-lag", "getJobLagHealth"],
  ["get", "/api/v1/health/storage-integrity", "getStorageIntegrityHealth"],
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
    "/api/v1/accounts/{account_id}/systems/{system_id}/inverters",
    "listInverters",
  ],
  [
    "post",
    "/api/v1/accounts/{account_id}/systems/{system_id}/inverters",
    "createInverter",
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

const sourceOperations = new Set();
for (const file of readdirSync("src/crates/api/src").filter((name) =>
  name.endsWith(".rs"),
)) {
  const source = readFileSync(`src/crates/api/src/${file}`, "utf8");
  for (const call of routeCalls(source)) {
    const path = call.match(/^\s*"([^"]+)"/)?.[1];
    if (path === undefined) continue;
    for (const method of call.matchAll(
      /(?:\.|\b)(get|post|put|patch|delete)\s*\(/g,
    )) {
      sourceOperations.add(`${method[1]} ${path}`);
    }
  }
}

const contractOperations = new Set();
let currentPath;
for (const line of spec.split("\n")) {
  const path = line.match(/^  (\/api\/[^:]+):$/)?.[1];
  if (path !== undefined) currentPath = path;
  const method = line.match(/^    (get|post|put|patch|delete):$/)?.[1];
  if (method !== undefined && currentPath !== undefined) {
    contractOperations.add(`${method} ${currentPath}`);
  }
}
for (const operation of sourceOperations) {
  if (!contractOperations.has(operation))
    throw new Error(`Axum route missing from OpenAPI: ${operation}`);
}
for (const operation of contractOperations) {
  if (!sourceOperations.has(operation))
    throw new Error(`OpenAPI operation missing from Axum: ${operation}`);
}
const operationIds = [...spec.matchAll(/^      operationId: (.+)$/gm)].map(
  (match) => match[1],
);
if (new Set(operationIds).size !== operationIds.length)
  throw new Error("duplicate OpenAPI operationId");
console.log(
  `Bidirectional Axum/OpenAPI coverage: ${sourceOperations.size} operations`,
);

function routeCalls(source) {
  const calls = [];
  let cursor = 0;
  while ((cursor = source.indexOf(".route(", cursor)) >= 0) {
    const start = cursor + ".route(".length;
    let depth = 1;
    let quoted = false;
    let escaped = false;
    let end = start;
    for (; end < source.length && depth > 0; end += 1) {
      const character = source[end];
      if (quoted) {
        if (escaped) escaped = false;
        else if (character === "\\") escaped = true;
        else if (character === '"') quoted = false;
      } else if (character === '"') quoted = true;
      else if (character === "(") depth += 1;
      else if (character === ")") depth -= 1;
    }
    calls.push(source.slice(start, end - 1));
    cursor = end;
  }
  return calls;
}
