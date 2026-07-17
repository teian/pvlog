import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const catalogPaths = [
  "assets/equipment-catalog/inverter-catalog-v1.json",
  "assets/equipment-catalog/pv-module-catalog-v1.json",
];

const [inverterCatalog, moduleCatalog] = catalogPaths.map((path) => {
  const document = readFileSync(path, "utf8");
  const catalog = JSON.parse(document);

  assert.notEqual(
    document.trim(),
    JSON.stringify(catalog),
    `${path} must not be stored as single-line JSON`,
  );
  assert.match(document, /^\{\n  "/, `${path} must use two-space indentation`);
  assert.ok(document.endsWith("\n"), `${path} must end with a newline`);

  return catalog;
});

assert.ok(!("solarModules" in inverterCatalog));
assert.ok(!("inverters" in moduleCatalog));

console.log("Equipment catalogs are split and pretty-printed");
