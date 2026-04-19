import { create } from "zustand";
import type { LoginResponse, User } from "@calibre/shared";

type AuthState = {
  access_token: string | null;
  refresh_token: string | null;
  user: User | null;
  setAuth: (auth: LoginResponse) => void;
  clearAuth: () => void;
};

export const AUTH_STORAGE_KEY = "calibre-web.auth";

type StoredAuth = Pick<LoginResponse, "access_token" | "refresh_token" | "user">;

function readStoredAuth(): StoredAuth | null {
  if (typeof localStorage === "undefined") {
    return null;
  }

  const raw = localStorage.getItem(AUTH_STORAGE_KEY);
  if (!raw) {
    return null;
  }

  try {
    const parsed = JSON.parse(raw) as Partial<StoredAuth>;
    if (
      typeof parsed.access_token === "string" &&
      typeof parsed.refresh_token === "string" &&
      parsed.user &&
      typeof parsed.user === "object"
    ) {
      return {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        user: parsed.user as User,
      };
    }
  } catch {
    return null;
  }

  return null;
}

function persistAuth(auth: StoredAuth | null): void {
  if (typeof localStorage === "undefined") {
    return;
  }

  if (!auth) {
    localStorage.removeItem(AUTH_STORAGE_KEY);
    return;
  }

  localStorage.setItem(AUTH_STORAGE_KEY, JSON.stringify(auth));
}

const initialAuth = readStoredAuth();

export const useAuthStore = create<AuthState>((set) => ({
  access_token: initialAuth?.access_token ?? null,
  refresh_token: initialAuth?.refresh_token ?? null,
  user: initialAuth?.user ?? null,
  setAuth: (auth) => {
    persistAuth(auth);
    set(auth);
  },
  clearAuth: () => {
    persistAuth(null);
    set({
      access_token: null,
      refresh_token: null,
      user: null,
    });
  },
}));
