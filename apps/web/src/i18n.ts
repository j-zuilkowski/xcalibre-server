import i18next, { type TFunction } from "i18next";
import { initReactI18next } from "react-i18next";

export const SUPPORTED_LANGUAGES = ["en", "fr", "de", "es"] as const;
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

const STORAGE_KEY = "autolibre.language";
const TRANSLATION_NAMESPACE = "translation";

function normalizeLanguage(value: string | null | undefined): SupportedLanguage {
  const candidate = value?.toLowerCase().split("-")[0];
  return SUPPORTED_LANGUAGES.includes(candidate as SupportedLanguage)
    ? (candidate as SupportedLanguage)
    : "en";
}

function readStoredLanguage(): SupportedLanguage | null {
  if (typeof localStorage === "undefined" || typeof localStorage.getItem !== "function") {
    return null;
  }

  return normalizeLanguage(localStorage.getItem(STORAGE_KEY));
}

function persistLanguage(language: SupportedLanguage) {
  if (typeof localStorage === "undefined" || typeof localStorage.setItem !== "function") {
    return;
  }

  localStorage.setItem(STORAGE_KEY, language);
}

function detectBrowserLanguage(): SupportedLanguage {
  if (typeof navigator === "undefined") {
    return "en";
  }

  return normalizeLanguage(navigator.language);
}

async function loadLanguageBundle(language: SupportedLanguage): Promise<void> {
  if (i18next.hasResourceBundle(language, TRANSLATION_NAMESPACE)) {
    return;
  }

  const response = await fetch(`/locales/${language}/translation.json`);
  if (!response.ok) {
    throw new Error(`Failed to load locale ${language}`);
  }

  const bundle = (await response.json()) as Record<string, unknown>;
  i18next.addResourceBundle(language, TRANSLATION_NAMESPACE, bundle, true, true);
}

export async function initializeI18n(): Promise<TFunction> {
  if (!i18next.isInitialized) {
    await i18next.use(initReactI18next).init({
      lng: "en",
      fallbackLng: "en",
      ns: [TRANSLATION_NAMESPACE],
      defaultNS: TRANSLATION_NAMESPACE,
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

  try {
    await loadLanguageBundle("en");
  } catch {
    // Fall back to parseMissingKeyHandler output if the English bundle is unavailable.
  }

  const preferredLanguage = readStoredLanguage() ?? detectBrowserLanguage();
  if (preferredLanguage !== "en") {
    try {
      await loadLanguageBundle(preferredLanguage);
    } catch {
      // Fall back to English if the bundle is unavailable.
    }
  }

  const nextLanguage = i18next.hasResourceBundle(preferredLanguage, TRANSLATION_NAMESPACE)
    ? preferredLanguage
    : "en";

  await i18next.changeLanguage(nextLanguage);
  persistLanguage(nextLanguage);

  return i18next.t.bind(i18next);
}

export async function changeLanguage(language: string): Promise<SupportedLanguage> {
  const nextLanguage = normalizeLanguage(language);
  await loadLanguageBundle(nextLanguage);
  await i18next.changeLanguage(nextLanguage);
  persistLanguage(nextLanguage);
  return nextLanguage;
}

export function getCurrentLanguage(): SupportedLanguage {
  return normalizeLanguage(i18next.language);
}

export default i18next;
