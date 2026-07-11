import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const checksums = readFileSync("release/checksums.txt", "utf8")
  .trim()
  .split("\n");
for (const line of checksums) {
  const [expected, file] = line.split(/\s+/, 2);
  const actual = createHash("sha256").update(readFileSync(file)).digest("hex");
  assert.equal(actual, expected, file);
}
const sbom = JSON.parse(readFileSync("release/pvlog-0.1.0.spdx.json", "utf8"));
assert.equal(sbom.spdxVersion, "SPDX-2.3");
assert.equal(sbom.packages[0].versionInfo, "0.1.0");
console.log(`Release artifacts: ${checksums.length} checksums and SPDX SBOM`);
