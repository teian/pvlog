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
  "docs/guides/system-setup.md",
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

// The setup guide names interface labels in bold. Each must be a string the
// English interface actually shows, so a renamed button fails this check
// instead of leaving the guide describing a screen that no longer exists.
const interfaceTexts = new Set();
(function collect(value) {
  if (typeof value === "string") interfaceTexts.add(value);
  else for (const child of Object.values(value)) collect(child);
})(
  JSON.parse(await readFile("src/ui/shared/lib/i18n/locales/en.json", "utf8")),
);
const setupGuide = await readFile("docs/guides/system-setup.md", "utf8");
const unknownLabels = [...setupGuide.matchAll(/\*\*([^*]+)\*\*/g)]
  .map((match) => match[1].replace(/\s+/g, " "))
  .filter((label) => !interfaceTexts.has(label));
if (unknownLabels.length > 0)
  throw new Error(
    `system-setup.md names labels the interface does not show: ${[...new Set(unknownLabels)].join(", ")}`,
  );

console.log(`Documentation conformance: ${documents.length} documents`);
