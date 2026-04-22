import * as SecureStore from "expo-secure-store";
import { ApiClient as CalibreClient, type RefreshResponse } from "@autolibre/shared";
import {
  getAccessTokenSync,
  getRefreshTokenSync,
  handleUnauthorized,
  hydrateAuthTokens,
  saveTokens,
} from "./auth";

const BASE_URL_KEY = "server_url";
const LEGACY_BASE_URL_KEY = "api_base_url";
const DEFAULT_BASE_URL = process.env.EXPO_PUBLIC_API_BASE_URL ?? "http://localhost:8080";

let baseUrl = DEFAULT_BASE_URL;
let initialized = false;
let client = createClient(baseUrl);

function createClient(url: string): CalibreClient {
  return new CalibreClient(
    url,
    () => getAccessTokenSync(),
    () => {
      handleUnauthorized();
    },
    {
      getRefreshToken: () => getRefreshTokenSync(),
      onRefreshTokens: (tokens: RefreshResponse) => {
        void saveTokens(tokens.access_token, tokens.refresh_token);
      },
    },
  );
}

export async function initializeApi(): Promise<void> {
  if (initialized) {
    return;
  }

  await hydrateAuthTokens();

  const storedBaseUrl =
    (await SecureStore.getItemAsync(BASE_URL_KEY)) ??
    (await SecureStore.getItemAsync(LEGACY_BASE_URL_KEY));
  if (storedBaseUrl && storedBaseUrl.trim().length > 0) {
    baseUrl = storedBaseUrl.trim();
    client = createClient(baseUrl);
  }

  initialized = true;
}

export async function getApiBaseUrl(): Promise<string> {
  if (!initialized) {
    await initializeApi();
  }
  return baseUrl;
}

export async function setApiBaseUrl(nextBaseUrl: string): Promise<void> {
  const normalized = nextBaseUrl.trim();

  if (normalized.length === 0) {
    baseUrl = DEFAULT_BASE_URL;
    await SecureStore.deleteItemAsync(BASE_URL_KEY);
    await SecureStore.deleteItemAsync(LEGACY_BASE_URL_KEY);
  } else {
    baseUrl = normalized.replace(/\/$/, "");
    await SecureStore.setItemAsync(BASE_URL_KEY, baseUrl);
    await SecureStore.deleteItemAsync(LEGACY_BASE_URL_KEY);
  }

  client = createClient(baseUrl);
  initialized = true;
}

export function useApi(): CalibreClient {
  return client;
}
