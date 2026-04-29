import type { Browser, BrowserContext, Page } from "@playwright/test";
import type { AuthSession, User } from "@xs/shared";
import { execFile } from "node:child_process";
import { promisify } from "node:util";

const API = process.env.PLAYWRIGHT_API_URL ?? "http://127.0.0.1:8083";
export const AUTH_STORAGE_KEY = "xcalibre.auth";
export const E2E_ADMIN_USERNAME = process.env.E2E_ADMIN_USERNAME ?? "admin";
export const E2E_ADMIN_PASSWORD = process.env.E2E_ADMIN_PASSWORD ?? "Test1234!";

let adminSessionPromise: Promise<AuthSession> | null = null;
let cachedAdminSession: AuthSession | null = null;
const execFileAsync = promisify(execFile);

type StatusError = Error & { status?: number };

function withStatus(message: string, status: number): StatusError {
  const error = new Error(message) as StatusError;
  error.status = status;
  return error;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms);
  });
}

async function postJson<T>(path: string, body: unknown): Promise<{ status: number; body: T | null }> {
  const { stdout } = await execFileAsync(
    "curl",
    [
      "-sS",
      "-H",
      "Content-Type: application/json",
      "-d",
      JSON.stringify(body),
      "-w",
      "\\n%{http_code}",
      `${API}${path}`,
    ],
    { encoding: "utf8", maxBuffer: 10 * 1024 * 1024 },
  );

  const lastNewline = stdout.lastIndexOf("\n");
  const payloadText = lastNewline >= 0 ? stdout.slice(0, lastNewline) : stdout;
  const statusText = lastNewline >= 0 ? stdout.slice(lastNewline + 1).trim() : "500";
  const status = Number(statusText);
  const bodyJson = payloadText.trim().length > 0 ? (JSON.parse(payloadText) as T) : null;

  return { status, body: bodyJson };
}

export async function createUser(username: string, password: string) {
  const adminSession = cachedAdminSession ?? (await ensureAdminAccount());
  const response = await fetch(`${API}/api/v1/admin/users`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${adminSession.access_token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      username,
      password,
      email: `${username}@test.local`,
      role_id: "user",
      is_active: true,
    }),
  });

  if (!response.ok && response.status !== 409) {
    throw new Error(`Failed to create user ${username}: ${response.status}`);
  }
}

export async function login(page: Page, username: string, password: string) {
  await page.goto("/login");
  await page.getByLabel("Username").fill(username);
  await page.getByLabel("Password").fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL("**/{home,library}", { waitUntil: "commit" });
}

export async function loginAsAdmin(page: Page) {
  await login(page, E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
}

export async function loginViaApi(username: string, password: string): Promise<AuthSession> {
  const response = await postJson<AuthSession>("/api/v1/auth/login", { username, password });

  if (response.status < 200 || response.status >= 300 || !response.body) {
    throw withStatus(`Failed to log in ${username}: ${response.status}`, response.status);
  }

  return response.body;
}

export async function ensureAdminAccount(): Promise<AuthSession> {
  if (!adminSessionPromise) {
    adminSessionPromise = (async () => {
      try {
        return await loginViaApi(E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
      } catch (error) {
        const status = (error as StatusError).status;
        if (status !== 401 && status !== 404) {
          await delay(250);
          return loginViaApi(E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
        }

        await createFirstAdmin();
        return loginViaApi(E2E_ADMIN_USERNAME, E2E_ADMIN_PASSWORD);
      }
    })();
  }

  try {
    return await adminSessionPromise;
  } catch (error) {
    adminSessionPromise = null;
    throw error;
  }
}

async function createFirstAdmin() {
  const response = await postJson<User>("/api/v1/auth/register", {
    username: E2E_ADMIN_USERNAME,
    password: E2E_ADMIN_PASSWORD,
    email: `${E2E_ADMIN_USERNAME}@test.local`,
  });

  if ((response.status < 200 || response.status >= 300) && response.status !== 409) {
    throw new Error(`Failed to create admin ${E2E_ADMIN_USERNAME}: ${response.status}`);
  }
}

export async function seedAuthState(context: BrowserContext, session: AuthSession) {
  await context.addInitScript(
    ({ authStorageKey, authSession }) => {
      localStorage.setItem(authStorageKey, JSON.stringify(authSession));
    },
    { authStorageKey: AUTH_STORAGE_KEY, authSession: session },
  );
}

export function makeUserSession(user: User, accessToken: string, refreshToken: string): AuthSession {
  return {
    user,
    access_token: accessToken,
    refresh_token: refreshToken,
  };
}

export async function bootstrapAdminSession(browser: Browser): Promise<AuthSession> {
  const page = await browser.newPage();
  try {
    await loginAsAdmin(page);
    const stored = await page.evaluate((storageKey) => localStorage.getItem(storageKey), AUTH_STORAGE_KEY);
    if (!stored) {
      throw new Error("Missing admin auth session");
    }

    const session = JSON.parse(stored) as AuthSession;
    cachedAdminSession = session;
    return session;
  } finally {
    await page.close();
  }
}
