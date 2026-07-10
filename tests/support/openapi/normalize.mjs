import { readFile } from "node:fs/promises";

import { parse } from "yaml";

function sortRecursively(value) {
  if (Array.isArray(value)) {
    return value.map(sortRecursively);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, item]) => [key, sortRecursively(item)]),
    );
  }
  return value;
}

/**
 * Parses and deterministically serializes an OpenAPI YAML document.
 *
 * @param {string} filePath - Contract path to normalize.
 * @returns {Promise<string>} Stable JSON representation.
 */
export async function normalizeOpenApi(filePath) {
  const source = await readFile(filePath, "utf8");
  return `${JSON.stringify(sortRecursively(parse(source)), null, 2)}\n`;
}
