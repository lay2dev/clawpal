import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import en from "./locales/en.json";

// English is bundled (fallback); Chinese is lazy-loaded on demand
const lazyLocales: Record<string, () => Promise<{ default: Record<string, string> }>> = {
  zh: () => import("./locales/zh.json"),
};

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
    },
    fallbackLng: "en",
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "clawpal_language",
      caches: ["localStorage"],
    },
  });

// Lazy-load detected language if not English
const detected = i18n.language?.split("-")[0];
if (detected && detected !== "en" && lazyLocales[detected]) {
  lazyLocales[detected]().then((mod) => {
    i18n.addResourceBundle(detected, "translation", mod.default, true, true);
    // Re-trigger resolution so components pick up the newly loaded bundle
    i18n.changeLanguage(detected);
  });
}

// Lazy-load on language change
i18n.on("languageChanged", (lng) => {
  const base = lng.split("-")[0];
  if (base !== "en" && lazyLocales[base] && !i18n.hasResourceBundle(base, "translation")) {
    lazyLocales[base]().then((mod) => {
      i18n.addResourceBundle(base, "translation", mod.default, true, true);
      // Re-trigger resolution so components pick up the newly loaded bundle
      i18n.changeLanguage(base);
    });
  }
});

export default i18n;
