import { readFileSync } from "node:fs";

const spec = readFileSync("openapi/pvlog-v1.yaml", "utf8");
const required = [
  ["post", "/api/v1/systems", "createSystem"], ["put", "/api/v1/systems/{id}", "updateSystem"], ["delete", "/api/v1/systems/{id}", "deleteSystem"],
  ["post", "/api/v1/systems/{id}/archive", "archiveSystem"], ["post", "/api/v1/systems/{id}/restore", "restoreSystem"],
  ["get", "/api/v1/accounts/{account_id}/systems/{system_id}/equipment", "listEquipment"], ["post", "/api/v1/accounts/{account_id}/systems/{system_id}/equipment", "createEquipment"],
  ["get", "/api/v1/accounts/{account_id}/systems/{system_id}/tariffs", "listTariffs"], ["post", "/api/v1/accounts/{account_id}/systems/{system_id}/tariffs", "createTariff"],
  ["get", "/api/v1/accounts/{account_id}/systems/{system_id}/channels", "listChannels"], ["post", "/api/v1/accounts/{account_id}/systems/{system_id}/channels", "createChannel"],
  ["get", "/api/v1/accounts/{account_id}/memberships", "listMemberships"], ["post", "/api/v1/accounts/{account_id}/memberships", "createMembership"],
  ["get", "/api/v1/accounts/{account_id}/credentials", "listCredentials"], ["post", "/api/v1/accounts/{account_id}/credentials", "createCredential"],
  ["post", "/api/v1/systems/{system_id}/observations", "createObservation"], ["post", "/api/v1/systems/{system_id}/observations/batch", "createObservationBatch"],
  ["patch", "/api/v1/systems/{system_id}/observations/{observation_id}", "correctObservation"], ["delete", "/api/v1/systems/{system_id}/observations/{observation_id}", "deleteObservation"],
];
for (const [method, path, operationId] of required) {
  const pathAt = spec.indexOf(`  ${path}:`);
  const nextPath = spec.indexOf("\n  /api/", pathAt + 1);
  const block = spec.slice(pathAt, nextPath < 0 ? spec.indexOf("\ncomponents:") : nextPath);
  if (pathAt < 0 || !block.includes(`    ${method}:`) || !block.includes(`operationId: ${operationId}`)) throw new Error(`missing ${method.toUpperCase()} ${path} (${operationId})`);
}
console.log(`OpenAPI route coverage: ${required.length} operations`);
