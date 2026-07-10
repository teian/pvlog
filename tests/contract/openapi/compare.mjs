import process from "node:process";

import { normalizeOpenApi } from "../../support/openapi/normalize.mjs";

const candidatePath = process.argv[2];
if (candidatePath === undefined) {
  throw new Error("Usage: compare.mjs <generated-openapi-path>");
}

const canonicalPath = "openapi/pvlog-v1.yaml";
const [canonical, candidate] = await Promise.all([
  normalizeOpenApi(canonicalPath),
  normalizeOpenApi(candidatePath),
]);

if (candidate !== canonical) {
  throw new Error(
    `Generated OpenAPI contract ${candidatePath} differs from ${canonicalPath}`,
  );
}
