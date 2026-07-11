import { access, readFile } from "node:fs/promises";
import path from "node:path";

const documents = [
  "README.md",
  "CHANGELOG.md",
  "SECURITY.md",
  "docs/README.md",
  "docs/guides/developer-quickstarts.md",
  "docs/guides/local-authentication-rbac.md",
  "docs/guides/operator-recovery.md",
  "docs/guides/uploader-integration.md",
  "docs/reference/api-domain-reference.md",
  "docs/architecture/telemetry-segment-format.md",
  "docs/operations/capacity-report.md",
  "docs/release/0.1.0.md",
];

for (const document of documents) {
  const content = await readFile(document, "utf8");
  if (!content.startsWith("# ")) throw new Error(`${document}: missing title`);
  for (const match of content.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)) {
    const target = match[1].split("#", 1)[0];
    if (target === "" || /^[a-z]+:/i.test(target)) continue;
    await access(path.resolve(path.dirname(document), target));
  }
}

const quickstart = await readFile(
  "docs/guides/developer-quickstarts.md",
  "utf8",
);
for (const term of [
  "Idempotency-Key",
  "application/problem+json",
  "Retry-After",
  "PKCE",
]) {
  if (!quickstart.includes(term)) throw new Error(`quickstart missing ${term}`);
}
const docsPage = await readFile("src/ui/pages/ApiReferencePage.tsx", "utf8");
for (const feature of ["api-version", "download", "/openapi/pvlog-v1.yaml"]) {
  if (!docsPage.includes(feature))
    throw new Error(`API docs UI missing ${feature}`);
}

console.log(`Documentation conformance: ${documents.length} documents`);
