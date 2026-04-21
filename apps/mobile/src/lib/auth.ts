import * as SecureStore from "expo-secure-store";

const ACCESS_TOKEN_KEY = "access_token";
const REFRESH_TOKEN_KEY = "refresh_token";

let cachedAccessToken: string | null = null;
let cachedRefreshToken: string | null = null;
let hydrated = false;
let authExpiredHandler: (() => void) | null = null;

export async function hydrateAuthTokens(): Promise<void> {
  if (hydrated) {
    return;
  }

  const [accessToken, refreshToken] = await Promise.all([
    SecureStore.getItemAsync(ACCESS_TOKEN_KEY),
    SecureStore.getItemAsync(REFRESH_TOKEN_KEY),
  ]);

  cachedAccessToken = accessToken;
  cachedRefreshToken = refreshToken;
  hydrated = true;
}

export async function saveTokens(access: string, refresh: string): Promise<void> {
  cachedAccessToken = access;
  cachedRefreshToken = refresh;
  hydrated = true;

  await Promise.all([
    SecureStore.setItemAsync(ACCESS_TOKEN_KEY, access),
    SecureStore.setItemAsync(REFRESH_TOKEN_KEY, refresh),
  ]);
}

export async function getAccessToken(): Promise<string | null> {
  if (!hydrated) {
    await hydrateAuthTokens();
  }
  return cachedAccessToken;
}

export async function getRefreshToken(): Promise<string | null> {
  if (!hydrated) {
    await hydrateAuthTokens();
  }
  return cachedRefreshToken;
}

export function getAccessTokenSync(): string | null {
  return cachedAccessToken;
}

export function getRefreshTokenSync(): string | null {
  return cachedRefreshToken;
}

export async function clearTokens(): Promise<void> {
  cachedAccessToken = null;
  cachedRefreshToken = null;
  hydrated = true;

  await Promise.all([
    SecureStore.deleteItemAsync(ACCESS_TOKEN_KEY),
    SecureStore.deleteItemAsync(REFRESH_TOKEN_KEY),
  ]);
}

export function setAuthExpiredHandler(handler?: () => void): void {
  authExpiredHandler = handler ?? null;
}

export function handleUnauthorized(): void {
  void clearTokens().finally(() => {
    authExpiredHandler?.();
  });
}
