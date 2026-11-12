import de from "@/shared/lib/i18n/locales/de.json";
import en from "@/shared/lib/i18n/locales/en.json";
import { describe, expect, it } from "vitest";

/**
 * Terms the interface no longer uses, per language.
 *
 * Each concept has exactly one name: a PV installation is a "system"
 * (Anlage), an uploader connection a "data source" (Datenquelle), a secret
 * for API access an "API key" (API-Schlüssel), a named value series a
 * "measurement channel" (Messkanal) and a webhook or mail target a
 * "notification target" (Benachrichtigungsziel). The patterns match the
 * other names these concepts used to carry.
 */
const RETIRED_TERMS: Record<"de" | "en", [term: string, pattern: RegExp][]> = {
  en: [
    ["plant", /\bplants?\b/iu],
    ["system aggregate", /system aggregate/iu],
    ["uploader", /uploader/iu],
    ["credential", /credential/iu],
    ["API-Key", /API-Key/u],
    ["channel (alone)", /(?<!measurement )\bchannels?\b/iu],
    ["catalogue", /catalogue/iu],
  ],
  de: [
    ["Anlagenaggregat", /Anlagenaggregat/iu],
    ["Uploader", /Uploader/iu],
    ["Credential", /Credential/iu],
    ["Zugangsdaten", /Zugangsdaten/iu],
    ["Zugang", /\bZug[aä]ng(?:e|en|s)?\b/u],
    ["API-Key", /API-Key/iu],
    ["Kanal (alone)", /(?<!Mess)kan[aä]l/iu],
  ],
};

/** A user-facing string that still uses a retired term. */
interface Finding {
  key: string;
  term: string;
}

/** Lists every string of a catalog that uses a retired term. */
function retiredTerms(
  language: "de" | "en",
  catalog: unknown,
  prefix = "",
): Finding[] {
  if (typeof catalog === "string")
    return RETIRED_TERMS[language]
      .filter(([, pattern]) => pattern.test(catalog))
      .map(([term]) => ({ key: prefix, term }));
  if (!catalog || typeof catalog !== "object") return [];
  return Object.entries(catalog).flatMap(([key, value]) =>
    retiredTerms(language, value, prefix ? `${prefix}.${key}` : key),
  );
}

describe("setup terminology", () => {
  it("uses no retired term in the English catalog", () => {
    expect(retiredTerms("en", en)).toEqual([]);
  });

  it("uses no retired term in the German catalog", () => {
    expect(retiredTerms("de", de)).toEqual([]);
  });

  it.each([
    ["en", "Copy this credential now.", "credential"],
    ["en", "Check the uploader.", "uploader"],
    ["en", "No channels are configured.", "channel (alone)"],
    ["en", "Inverter catalogue", "catalogue"],
    ["de", "API-Key erstellen", "API-Key"],
    ["de", "Kopiere diesen Zugang jetzt.", "Zugang"],
    ["de", "Benachrichtigungskanäle", "Kanal (alone)"],
    ["de", "Prüfe das Anlagenaggregat.", "Anlagenaggregat"],
  ] as const)(
    "fails on a seeded retired term (%s: %s)",
    (language, text, term) => {
      expect(
        retiredTerms(language, { seeded: { label: text } }),
      ).toContainEqual({ key: "seeded.label", term });
    },
  );

  it.each([
    ["en", "2 measurement channels deliver values."],
    ["en", "Notification targets"],
    ["de", "Messkanäle erscheinen hier."],
    ["de", "Diese Integration verliert sofort den Zugriff."],
  ] as const)("accepts the current terms (%s: %s)", (language, text) => {
    expect(retiredTerms(language, { label: text })).toEqual([]);
  });
});
