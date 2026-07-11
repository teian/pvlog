#!/usr/bin/env node

import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const IGNORED_DIRECTORIES = new Set([
  ".git",
  ".cache",
  ".codex",
  ".claude",
  "dist",
  "node_modules",
  "target",
]);
const ROOT_UI_TOOLING = new Set([
  "eslint.config.js",
  "playwright.config.ts",
  "vite.config.ts",
  "vitest.config.ts",
]);
const UI_EXTENSIONS = new Set([".css", ".js", ".jsx", ".mjs", ".ts", ".tsx"]);
const TEST_FILE_PATTERN =
  /(?:\.(?:spec|test)\.[^.]+$|^(?:test_|tests?\.)[^/]+$)/u;
const TEST_DIRECTORY_PATTERN =
  /(?:^|\/)(?:__tests__|fixtures|test-support)(?:\/|$)/u;

/**
 * Validates the repository's production and test source boundaries.
 *
 * @param {string} repositoryRoot - Absolute or relative repository root.
 * @returns {string[]} Human-readable policy violations.
 */
export function validateStructure(repositoryRoot) {
  const root = path.resolve(repositoryRoot);
  const files = collectFiles(root);
  const violations = [];

  for (const absolutePath of files) {
    const relativePath = normalize(path.relative(root, absolutePath));
    const extension = path.extname(relativePath);
    const insideTests = isInside(relativePath, "tests");
    const insideBackend = isInside(relativePath, "src/crates");
    const insideUi = isInside(relativePath, "src/ui");
    const insideDocumentation = isInside(relativePath, "docs");

    if (extension === ".rs" && !insideBackend && !insideTests) {
      violations.push(
        `Rust production source must be under src/crates/: ${relativePath}`,
      );
    }

    if (
      UI_EXTENSIONS.has(extension) &&
      !insideUi &&
      !insideTests &&
      !insideDocumentation &&
      !ROOT_UI_TOOLING.has(relativePath)
    ) {
      violations.push(
        `UI production source must be under src/ui/: ${relativePath}`,
      );
    }

    if (
      !insideTests &&
      (TEST_FILE_PATTERN.test(path.basename(relativePath)) ||
        TEST_DIRECTORY_PATTERN.test(relativePath))
    ) {
      violations.push(`Test-only code must be under tests/: ${relativePath}`);
    }

    if (insideBackend && extension === ".rs") {
      inspectBackendSource(absolutePath, relativePath, violations);
    }
    if (insideUi && UI_EXTENSIONS.has(extension)) {
      inspectUiSource(absolutePath, relativePath, violations);
    }
    if (insideBackend && path.basename(relativePath) === "Cargo.toml") {
      inspectCargoManifest(absolutePath, relativePath, violations);
    }
  }

  if (
    files.some((file) =>
      isInside(normalize(path.relative(root, file)), "src/ui/src"),
    )
  ) {
    violations.push("Nested src/ui/src/ source roots are prohibited");
  }

  return [...new Set(violations)].sort();
}

function collectFiles(root) {
  const files = [];
  const visit = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (entry.isDirectory() && IGNORED_DIRECTORIES.has(entry.name)) {
        continue;
      }
      const entryPath = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        visit(entryPath);
      } else if (entry.isFile()) {
        files.push(entryPath);
      }
    }
  };
  visit(root);
  return files;
}

function inspectBackendSource(absolutePath, relativePath, violations) {
  const source = readFileSync(absolutePath, "utf8");
  if (/#\s*\[cfg(?:_attr)?\s*\(\s*test\b/u.test(source)) {
    violations.push(
      `Inline Rust test code must move to tests/: ${relativePath}`,
    );
  }
  if (/^\s*(?:use|mod)\s+[^;]*\btests?\b/mu.test(source)) {
    violations.push(
      `Production Rust source must not depend on tests/: ${relativePath}`,
    );
  }
  if (/\binclude(?:_str|_bytes)?!\s*\([^)]*tests\//u.test(source)) {
    violations.push(
      `Production Rust source must not include tests/: ${relativePath}`,
    );
  }
}

function inspectUiSource(absolutePath, relativePath, violations) {
  const source = readFileSync(absolutePath, "utf8");
  if (/\b(?:from\s+|import\s*\()["'][^"']*(?:^|\/)tests\//mu.test(source)) {
    violations.push(
      `Production UI source must not import tests/: ${relativePath}`,
    );
  }
}

function inspectCargoManifest(absolutePath, relativePath, violations) {
  const source = readFileSync(absolutePath, "utf8");
  let dependencySection = false;
  for (const line of source.split(/\r?\n/u)) {
    const section = /^\s*\[([^\u005D]+)\]\s*$/u.exec(line)?.[1];
    if (section !== undefined) {
      dependencySection = /(?:^|\.)dependencies$/u.test(section);
      continue;
    }
    if (
      dependencySection &&
      /(?:path\s*=\s*["'][^"']*tests(?:\/|["'])|^\s*tests?\s*=)/u.test(line)
    ) {
      violations.push(
        `Production Cargo dependencies must not reference tests/: ${relativePath}`,
      );
    }
  }
}

function isInside(relativePath, directory) {
  return relativePath === directory || relativePath.startsWith(`${directory}/`);
}

function normalize(filePath) {
  return filePath.split(path.sep).join("/");
}

const invokedPath = process.argv[1];
if (
  invokedPath !== undefined &&
  path.resolve(invokedPath) === fileURLToPath(import.meta.url)
) {
  const repositoryRoot = process.argv[2] ?? process.cwd();
  const violations = validateStructure(repositoryRoot);
  if (violations.length > 0) {
    console.error("Repository structure violations:");
    for (const violation of violations) {
      console.error(`- ${violation}`);
    }
    process.exitCode = 1;
  } else {
    console.log("Repository structure is valid.");
  }
}
