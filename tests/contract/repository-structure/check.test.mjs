import assert from "node:assert/strict";
import { mkdirSync, writeFileSync } from "node:fs";
import { afterEach, test } from "node:test";
import path from "node:path";

import { validateStructure } from "./check.mjs";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";

const roots = [];

afterEach(() => {
  for (const root of roots.splice(0)) {
    rmSync(root, { recursive: true, force: true });
  }
});

test("accepts the required source and test roots", () => {
  const root = fixture({
    "src/crates/api/src/lib.rs": "pub fn router() {}\n",
    "src/ui/main.tsx": "export const application = true;\n",
    "tests/rust/api.rs": "#[test]\nfn routes() {}\n",
    "tests/ui/application.test.tsx": "export const testOnly = true;\n",
    "scripts/import-catalog.mjs": "export const importCatalog = true;\n",
    "vite.config.ts": "export default {};\n",
  });

  assert.deepEqual(validateStructure(root), []);
});

test("rejects backend and UI production source outside their roots", () => {
  const root = fixture({
    "backend/main.rs": "fn main() {}\n",
    "frontend/main.tsx": "export const application = true;\n",
  });

  assert.match(validateStructure(root).join("\n"), /src\/crates/u);
  assert.match(validateStructure(root).join("\n"), /src\/ui/u);
});

test("rejects nested UI roots and test-only production code", () => {
  const root = fixture({
    "src/crates/api/src/lib.rs": "#[cfg(test)]\nmod tests {}\n",
    "src/ui/src/App.test.tsx": "export const testOnly = true;\n",
  });
  const result = validateStructure(root).join("\n");

  assert.match(result, /Nested src\/ui\/src/u);
  assert.match(result, /Inline Rust test code/u);
  assert.match(result, /Test-only code must be under tests/u);
});

test("rejects production dependencies on centralized tests", () => {
  const root = fixture({
    "src/crates/api/Cargo.toml":
      '[dependencies]\nfakes = { path = "../../../tests/support/fakes" }\n',
    "src/crates/api/src/lib.rs":
      'include!("../../../../tests/support/fake.rs");\n',
    "src/ui/main.ts": 'import { fake } from "../../tests/support/fake";\n',
  });
  const result = validateStructure(root).join("\n");

  assert.match(result, /Cargo dependencies/u);
  assert.match(result, /Production Rust source/u);
  assert.match(result, /Production UI source/u);
});

function fixture(files) {
  const root = mkdtempSync(path.join(tmpdir(), "pvlog-structure-"));
  roots.push(root);
  for (const [relativePath, content] of Object.entries(files)) {
    const absolutePath = path.join(root, relativePath);
    mkdirSync(path.dirname(absolutePath), { recursive: true });
    writeFileSync(absolutePath, content);
  }
  return root;
}
