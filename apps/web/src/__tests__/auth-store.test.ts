import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";

function createLocalStorageMock() {
  const store = new Map<string, string>();

  return {
    getItem: vi.fn((key: string) => store.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => {
      store.set(key, value);
    }),
    removeItem: vi.fn((key: string) => {
      store.delete(key);
    }),
    clear: vi.fn(() => {
      store.clear();
    }),
    key: vi.fn((index: number) => Array.from(store.keys())[index] ?? null),
    get length() {
      return store.size;
    },
  } as Storage;
}

let localStorageMock: Storage;

beforeEach(() => {
  vi.resetModules();
  localStorageMock = createLocalStorageMock();
  Object.defineProperty(globalThis, "localStorage", {
    value: localStorageMock,
    configurable: true,
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("auth store", () => {
  test("test_set_auth_persists_to_storage", async () => {
    const { useAuthStore, AUTH_STORAGE_KEY } = await import("../lib/auth-store");
    const auth = {
      access_token: "access-token",
      refresh_token: "refresh-token",
      user: {
        id: "user-1",
        username: "alice",
        email: "alice@example.com",
        role: {
          id: "role-1",
          name: "admin",
        },
        is_active: true,
        force_pw_reset: false,
        default_library_id: "default",
        totp_enabled: false,
        created_at: "2026-04-18T00:00:00Z",
        last_modified: "2026-04-18T00:00:00Z",
      },
    };

    useAuthStore.getState().setAuth(auth);

    expect(localStorageMock.setItem).toHaveBeenCalledWith(
      AUTH_STORAGE_KEY,
      JSON.stringify(auth),
    );
    expect(useAuthStore.getState()).toMatchObject(auth);
  });

  test("test_clear_auth_removes_from_storage", async () => {
    const { useAuthStore, AUTH_STORAGE_KEY } = await import("../lib/auth-store");

    useAuthStore.getState().clearAuth();

    expect(localStorageMock.removeItem).toHaveBeenCalledWith(AUTH_STORAGE_KEY);
    expect(useAuthStore.getState()).toMatchObject({
      access_token: null,
      refresh_token: null,
      user: null,
    });
  });

  test("test_auth_restored_on_init", async () => {
    const auth = {
      access_token: "restored-access",
      refresh_token: "restored-refresh",
      user: {
        id: "user-1",
        username: "alice",
        email: "alice@example.com",
        role: {
          id: "role-1",
          name: "admin",
        },
        is_active: true,
        force_pw_reset: false,
        default_library_id: "default",
        totp_enabled: false,
        created_at: "2026-04-18T00:00:00Z",
        last_modified: "2026-04-18T00:00:00Z",
      },
    };

    localStorageMock.setItem("xcalibre.auth", JSON.stringify(auth));

    const { useAuthStore } = await import("../lib/auth-store");

    expect(useAuthStore.getState()).toMatchObject(auth);
  });
});
