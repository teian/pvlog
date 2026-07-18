import { readFile } from "node:fs/promises";

import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const catalogDirectory = new URL(
  "../assets/equipment-catalog/",
  import.meta.url,
);
const definitions = await readJson(
  "equipment-catalog-definitions-v1.schema.json",
);
const catalogs = [
  ["inverter-catalog-v1.schema.json", "inverter-catalog-v1.json"],
  ["pv-module-catalog-v1.schema.json", "pv-module-catalog-v1.json"],
];

const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
ajv.addSchema(definitions);

for (const [schemaFile, catalogFile] of catalogs) {
  const schema = await readJson(schemaFile);
  const catalog = await readJson(catalogFile);
  const validate = ajv.compile(schema);
  if (!validate(catalog)) {
    throw new Error(
      `${catalogFile} is invalid:\n${ajv.errorsText(validate.errors, { separator: "\n" })}`,
    );
  }
  console.log(`${catalogFile} valid`);
}

async function readJson(file) {
  return JSON.parse(await readFile(new URL(file, catalogDirectory), "utf8"));
}
