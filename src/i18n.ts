import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import de from "./locales/de.json";
import en from "./locales/en.json";
import { getLanguage } from "./preferences";

export const resources = { en: { translation: en }, de: { translation: de } };

i18n.use(initReactI18next).init({
  resources,
  lng: getLanguage(),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});
document.documentElement.lang = i18n.language;

export default i18n;
