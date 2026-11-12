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
  "docs/guides/ingestion-api.md",
  "docs/guides/smalog-export.md",
  "docs/reference/api-domain-reference.md",
  "docs/reference/pvoutput-field-mapping.md",
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

// The PVOutput mapping has to name every field of PVOutput's status, output
// and system services, and every PVLog field it points to has to exist in
// the API contract.
const mapping = await readFile(
  "docs/reference/pvoutput-field-mapping.md",
  "utf8",
);
const pvoutputFields = {
  status: [
    "d",
    "t",
    "v1",
    "v2",
    "v3",
    "v4",
    "v5",
    "v6",
    "c1",
    "n",
    "v7",
    "v12",
    "m1",
  ],
  output: [
    "d",
    "g",
    "e",
    "pp",
    "pt",
    "cd",
    "tm",
    "tx",
    "cm",
    "ip",
    "io",
    "is",
    "ih",
    "c",
  ],
  system: [
    "System name",
    "System size",
    "Postcode",
    "Number of panels",
    "Panel power",
    "Panel brand",
    "Number of inverters",
    "Inverter power",
    "Inverter brand",
    "Orientation",
    "Array tilt",
    "Shade",
    "Install date",
    "Latitude, longitude",
    "Status interval",
    "Secondary array",
    "Export tariff",
    "Import tariffs by band",
    "Daily charge",
  ],
};
const mappingSection = (title) => {
  const start = mapping.indexOf(`## ${title}`);
  const end = mapping.indexOf("\n## ", start + 1);
  return mapping.slice(start, end === -1 ? undefined : end);
};
for (const [section, title, wrap] of [
  ["status", "Status", (field) => `\`${field}\``],
  ["output", "Output", (field) => `\`${field}\``],
  ["system", "System", (field) => `| ${field} `],
]) {
  const text = mappingSection(title);
  for (const field of pvoutputFields[section]) {
    if (!text.includes(wrap(field)))
      throw new Error(
        `pvoutput-field-mapping.md: ${section} field ${field} is missing`,
      );
  }
}
const contract = await readFile("openapi/pvlog-v1.yaml", "utf8");
for (const [, field] of mapping.matchAll(
  /`([a-z][A-Za-z]+(?:\.[A-Za-z]+)?)`/g,
)) {
  // Short lowercase names are PVOutput's own parameters, not PVLog fields.
  if (
    !/[A-Z]/.test(field) &&
    ![
      "name",
      "channels",
      "message",
      "shading",
      "manufacturer",
      "model",
      "complete",
      "export",
      "import",
    ].includes(field)
  )
    continue;
  const name = field.split(".").at(-1);
  if (!new RegExp(`\\b${name}\\b`).test(contract))
    throw new Error(
      `pvoutput-field-mapping.md names ${field}, which the API contract does not define`,
    );
}

// The smalog mapping has to name every plant column of smalog's schema
// version 1, and every member it maps a column to has to be a member of the
// ingestion request in the API contract.
const smalogMapping = await readFile("docs/guides/smalog-export.md", "utf8");
const smalogSchema = JSON.parse(
  await readFile("tests/fixtures/ingest/smalog-schema-v1.columns.json", "utf8"),
);
const mappedMembers = new Set();
for (const [table, columns] of Object.entries(smalogSchema.tables)) {
  for (const column of columns) {
    const row = smalogMapping
      .split("\n")
      .find((line) => line.startsWith(`| \`${table}.${column}\` `));
    if (!row)
      throw new Error(`smalog-export.md: column ${table}.${column} is missing`);
    const target = row.split("|")[2].trim();
    if (target === "not sent") continue;
    for (const [, member] of target.matchAll(/`([^`]+)`/g))
      mappedMembers.add(member.split(".").at(-1).replace(/\[.*$/, ""));
  }
}
const ingestSchemas = contract.slice(
  contract.indexOf("\n    IngestRequest:"),
  contract.indexOf("\n    IngestRejection:"),
);
for (const member of mappedMembers) {
  if (
    !new RegExp(`\\b${member}:`).test(ingestSchemas) &&
    !contract.includes(`\n        ${member}:`)
  )
    throw new Error(
      `smalog-export.md maps a column to ${member}, which the ingestion request does not define`,
    );
}

// The contract fixture is the request both repositories test against, so it
// has to be a valid request of the documented schema.
const { default: Ajv2020 } = await import("ajv/dist/2020.js");
const { default: addFormats } = await import("ajv-formats");
const { parse } = await import("yaml");
const ajv = new Ajv2020({ strict: false, allErrors: true });
addFormats(ajv);
const validateIngest = ajv.compile({
  $ref: "#/components/schemas/IngestRequest",
  components: parse(contract).components,
});
const ingestFixture = JSON.parse(
  await readFile("tests/fixtures/ingest/smalog-schema-v1.request.json", "utf8"),
);
if (!validateIngest(ingestFixture))
  throw new Error(
    `smalog-schema-v1.request.json does not match IngestRequest: ${ajv.errorsText(validateIngest.errors)}`,
  );
// The schema refuses what the server refuses: a member it does not define.
const withUnknownMember = structuredClone(ingestFixture);
withUnknownMember.measurements[0].devices[0].powerWatts = 1;
if (validateIngest(withUnknownMember))
  throw new Error("IngestRequest accepts a member it does not define");

console.log(`Documentation conformance: ${documents.length} documents`);
