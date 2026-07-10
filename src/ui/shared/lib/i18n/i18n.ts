import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";

import de from "./locales/de.json";
import en from "./locales/en.json";

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      de: { translation: de },
      en: { translation: en },
    },
    supportedLngs: ["en", "de"],
    fallbackLng: "en",
    ns: ["translation"],
    defaultNS: "translation",
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      caches: ["localStorage"],
      lookupLocalStorage: "pvlog-language",
    },
  });

function synchronizeDocumentLanguage(language: string) {
  document.documentElement.lang = language.split("-", 1)[0] ?? "en";
}

synchronizeDocumentLanguage(i18n.resolvedLanguage ?? i18n.language);
i18n.on("languageChanged", synchronizeDocumentLanguage);

export default i18n;
