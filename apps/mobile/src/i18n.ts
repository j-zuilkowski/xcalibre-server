import i18next from "i18next";
import { initReactI18next } from "react-i18next";
import * as Localize from "react-native-localize";
import en from "./locales/en/translation.json";
import fr from "./locales/fr/translation.json";
import de from "./locales/de/translation.json";
import es from "./locales/es/translation.json";

export const SUPPORTED_LANGUAGES = ["en", "fr", "de", "es"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

const resources = {
  en: { translation: en },
  fr: { translation: fr },
  de: { translation: de },
  es: { translation: es },
} as const;

function normalizeLanguage(value: string | null | undefined): SupportedLanguage {
  const candidate = value?.toLowerCase().split("-")[0];
  return SUPPORTED_LANGUAGES.includes(candidate as SupportedLanguage)
    ? (candidate as SupportedLanguage)
    : "en";
}

function detectLanguage(): SupportedLanguage {
  const locales = Localize.getLocales();
  return normalizeLanguage(locales[0]?.languageCode ?? locales[0]?.languageTag);
}

export async function initializeI18n() {
  if (!i18next.isInitialized) {
    await i18next.use(initReactI18next).init({
      lng: detectLanguage(),
      fallbackLng: "en",
      resources,
      ns: ["translation"],
      defaultNS: "translation",
      interpolation: {
        escapeValue: false,
      },
      returnNull: false,
      returnEmptyString: false,
      parseMissingKeyHandler: (key) => key,
      react: {
        useSuspense: false,
      },
    });
  }

  return i18next;
}

export async function changeLanguage(language: string): Promise<SupportedLanguage> {
  const nextLanguage = normalizeLanguage(language);
  await i18next.changeLanguage(nextLanguage);
  return nextLanguage;
}

export default i18next;
