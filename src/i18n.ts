import i18n, { type Callback } from "i18next";
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

// Intercept changeLanguage to pre-load lazy bundles before the switch.
// This ensures the bundle is available when i18next fires "languageChanged",
// so React components render translated text on the first pass.
const _originalChangeLanguage = i18n.changeLanguage.bind(i18n);
i18n.changeLanguage = async (lng?: string, callback?: Callback) => {
  if (lng) {
    const base = lng.split("-")[0];
    if (base !== "en" && lazyLocales[base] && !i18n.hasResourceBundle(base, "translation")) {
      const mod = await lazyLocales[base]();
      i18n.addResourceBundle(base, "translation", mod.default, true, true);
    }
  }
  return _originalChangeLanguage(lng, callback);
};

// Eager-load detected language on startup (e.g. persisted clawpal_language=zh)
const detected = i18n.language?.split("-")[0];
if (detected && detected !== "en" && lazyLocales[detected]) {
  // Use our wrapped changeLanguage so the bundle loads before the switch
  i18n.changeLanguage(detected);
}

export default i18n;
