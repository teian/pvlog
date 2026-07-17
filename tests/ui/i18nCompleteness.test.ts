import de from "@/shared/lib/i18n/locales/de.json";
import en from "@/shared/lib/i18n/locales/en.json";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/** Flattens translation leaves into dot-separated keys. */
function translationKeys(
  value: unknown,
  prefix = "",
  result = new Set<string>(),
): Set<string> {
  if (!value || typeof value !== "object") return result;
  for (const [key, child] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (child && typeof child === "object")
      translationKeys(child, path, result);
    else result.add(path);
  }
  return result;
}

/** Recursively collects TypeScript UI implementation files. */
function uiFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return uiFiles(path);
    return /\.[jt]sx?$/u.test(entry.name) ? [path] : [];
  });
}

/** Extracts statically referenced translation keys from one source file. */
function referencedKeys(file: string): string[] {
  const source = readFileSync(file, "utf8");
  return [...source.matchAll(/\bt\(\s*["']([^"']+)["']/gu)].map(
    (match) => match[1] ?? "",
  );
}

function containsKey(catalog: Set<string>, key: string): boolean {
  return (
    catalog.has(key) ||
    (catalog.has(`${key}_one`) && catalog.has(`${key}_other`))
  );
}

describe("German and English translation completeness", () => {
  const germanKeys = translationKeys(de);
  const englishKeys = translationKeys(en);

  it("keeps both locale catalogs structurally identical", () => {
    expect([...germanKeys].sort()).toEqual([...englishKeys].sort());
  });

  it("provides both languages for every statically referenced key", () => {
    const keys = new Set(uiFiles("src/ui").flatMap(referencedKeys));
    const missing = [...keys].filter(
      (key) => !containsKey(germanKeys, key) || !containsKey(englishKeys, key),
    );
    expect(missing).toEqual([]);
  });

  it("localizes critical navigation and reporting labels", () => {
    expect(de.nav).toMatchObject({
      allSystems: "Alle Anlagen",
      dashboard: "Übersicht",
      statistics: "Statistik",
      systems: "Anlagen",
      weather: "Wetter",
    });
    expect(de.reporting).toMatchObject({
      statistics: { title: "Statistik" },
      seasonal: { title: "Jahreszeiten" },
      weather: { title: "Wetter" },
    });
    expect(en.nav).toMatchObject({
      allSystems: "All Systems",
      dashboard: "Dashboard",
      statistics: "Statistics",
      systems: "Systems",
      weather: "Weather",
    });
    expect(de.administration.navigation.label).toBe("Verwaltung");
    expect(en.administration.navigation.label).toBe("Administration");
    expect(de.accountApiKeys.title).toBe("Konto-API-Keys");
    expect(en.accountApiKeys.title).toBe("Account API keys");
  });

  it("localizes known backend metadata in both catalogs", () => {
    expect(de.administration.audit.actions.account_api_key_issue).toBe(
      "API-Key erstellen",
    );
    expect(en.administration.audit.actions.account_api_key_issue).toBe(
      "Create API key",
    );
    expect(de.administration.roles.kinds.built_in).toBe("Integriert");
    expect(en.administration.roles.kinds.built_in).toBe("Built-in");
    expect(de.reporting.lifecycle.archived).toBe("Archiviert");
    expect(en.reporting.lifecycle.archived).toBe("Archived");
  });
});
