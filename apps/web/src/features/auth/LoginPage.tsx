/**
 * LoginPage — credential entry and TOTP challenge form.
 *
 * Route: /login  (public, no ProtectedRoute wrapper)
 *
 * Two-step login flow:
 *   Step 1 ("credentials"): username + password form.
 *     - Calls POST /api/v1/auth/login.
 *     - If the response contains `totp_required`, the server has issued a
 *       short-lived `totp_token` — the UI transitions to step 2.
 *     - Otherwise the full token pair is stored via `useAuthStore.setAuth`
 *       and the user is navigated to /library.
 *
 *   Step 2 ("totp"): 6-digit authenticator code or 8-char backup code.
 *     - Calls POST /api/v1/auth/totp/verify or .../totp/verify-backup.
 *     - Input autofocuses on transition to this step.
 *     - Toggle between authenticator and backup code modes without leaving
 *       the step.
 *
 * OAuth buttons: rendered only when GET /api/v1/auth/providers returns
 * `google: true` or `github: true`.  Each button is a plain <a> pointing to
 * the backend OAuth initiation endpoint so the browser follows the redirect.
 *
 * API calls:
 *   GET  /api/v1/auth/providers
 *   POST /api/v1/auth/login
 *   POST /api/v1/auth/totp/verify
 *   POST /api/v1/auth/totp/verify-backup
 */
import { useEffect, useRef, useState, type CSSProperties, type FormEvent } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import type { ApiError } from "@xs/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";

function getErrorMessage(error: unknown, fallback: string, t: (key: string) => string): string {
  if (typeof error === "object" && error && "status" in error) {
    const apiError = error as ApiError;
    if (apiError.status === 401) {
      return t("auth.invalid_credentials");
    }
    return apiError.message || fallback;
  }

  return fallback;
}

const pageStyle: CSSProperties = {
  minHeight: "100vh",
  display: "grid",
  placeItems: "center",
  padding: "24px",
  background:
    "radial-gradient(circle at top, rgba(15, 118, 110, 0.18), transparent 40%), linear-gradient(135deg, #fafafa 0%, #f4f4f5 45%, #e4e4e7 100%)",
  color: "#18181b",
  fontFamily: "Inter, system-ui, sans-serif",
};

const cardStyle: CSSProperties = {
  width: "100%",
  maxWidth: "420px",
  borderRadius: "24px",
  background: "rgba(255, 255, 255, 0.92)",
  border: "1px solid rgba(228, 228, 231, 0.9)",
  boxShadow: "0 24px 60px rgba(24, 24, 27, 0.12)",
  padding: "32px",
  backdropFilter: "blur(18px)",
};

const labelStyle: CSSProperties = {
  display: "block",
  fontSize: "14px",
  fontWeight: 600,
  marginBottom: "6px",
};

const inputStyle: CSSProperties = {
  width: "100%",
  borderRadius: "14px",
  border: "1px solid #d4d4d8",
  background: "#fff",
  padding: "12px 14px",
  fontSize: "15px",
  outline: "none",
  boxSizing: "border-box",
};

/**
 * LoginPage renders the two-step authentication form (credentials → TOTP)
 * and optional OAuth provider buttons.
 */
export function LoginPage() {
  const navigate = useNavigate();
  const setAuth = useAuthStore((state) => state.setAuth);
  const { t } = useTranslation();
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [totpToken, setTotpToken] = useState("");
  const [totpCode, setTotpCode] = useState("");
  const [useBackupCode, setUseBackupCode] = useState(false);
  const [totpError, setTotpError] = useState<string | null>(null);
  const [step, setStep] = useState<"credentials" | "totp">("credentials");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const totpInputRef = useRef<HTMLInputElement | null>(null);
  const errorId = "login-form-error";
  const totpErrorId = "login-totp-error";

  const providersQuery = useQuery({
    queryKey: ["auth-providers"],
    queryFn: () => apiClient.getAuthProviders(),
    staleTime: 5 * 60_000,
  });

  useEffect(() => {
    if (step === "totp") {
      totpInputRef.current?.focus();
    }
  }, [step, useBackupCode]);

  async function handleVerifyTotp() {
    setIsSubmitting(true);
    setTotpError(null);

    try {
      const response = useBackupCode
        ? await apiClient.verifyTotpBackup(totpToken, totpCode.trim())
        : await apiClient.verifyTotp(totpToken, totpCode.trim());
      setAuth(response);
      await navigate({ to: "/library" });
    } catch {
      setTotpError("Invalid code");
    } finally {
      setIsSubmitting(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (step === "totp") {
      await handleVerifyTotp();
      return;
    }

    setIsSubmitting(true);
    setError(null);

    try {
      const response = await apiClient.login({ username, password });
      if ("totp_required" in response) {
        setTotpToken(response.totp_token);
        setTotpCode("");
        setUseBackupCode(false);
        setTotpError(null);
        setError(null);
        setStep("totp");
        return;
      }

      setAuth(response);
      await navigate({ to: "/library" });
    } catch (err) {
      setError(getErrorMessage(err, t("auth.unable_to_sign_in"), t));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <main style={pageStyle}>
      <section style={cardStyle}>
        <p style={{ margin: 0, color: "#0f766e", fontSize: "13px", fontWeight: 700, letterSpacing: "0.08em", textTransform: "uppercase" }}>
          {t("app_name")}
        </p>
        <h1 style={{ margin: "10px 0 8px", fontSize: "34px", lineHeight: 1.1 }}>{t("auth.login_title")}</h1>
        <p style={{ margin: "0 0 24px", color: "#52525b", fontSize: "15px" }}>
          {t("auth.login_subtitle")}
        </p>

        <form onSubmit={handleSubmit} style={{ display: "grid", gap: "18px" }}>
          {step === "credentials" ? (
            <>
              <div>
                <label htmlFor="username" style={labelStyle}>
                  {t("auth.username")}
                </label>
                <input
                  id="username"
                  name="username"
                  autoComplete="username"
                  value={username}
                  onChange={(event) => setUsername(event.target.value)}
                  aria-describedby={error ? errorId : undefined}
                  aria-invalid={Boolean(error)}
                  style={inputStyle}
                />
              </div>
              <div>
                <label htmlFor="password" style={labelStyle}>
                  {t("auth.password")}
                </label>
                <input
                  id="password"
                  name="password"
                  type="password"
                  autoComplete="current-password"
                  value={password}
                  onChange={(event) => setPassword(event.target.value)}
                  aria-describedby={error ? errorId : undefined}
                  aria-invalid={Boolean(error)}
                  style={inputStyle}
                />
              </div>
            </>
          ) : (
            <div style={{ display: "grid", gap: "14px" }}>
              <div>
                <p style={{ margin: 0, color: "#0f766e", fontSize: "13px", fontWeight: 700, letterSpacing: "0.08em", textTransform: "uppercase" }}>
                  Two-factor authentication
                </p>
                <h2 style={{ margin: "8px 0 4px", fontSize: "26px", lineHeight: 1.15 }}>
                  {useBackupCode ? "Backup code" : "Verification code"}
                </h2>
                <p style={{ margin: 0, color: "#52525b", fontSize: "14px" }}>
                  Enter the six-digit code from your authenticator app.
                </p>
              </div>

              <div>
                <label htmlFor="totp-code" style={labelStyle}>
                  {useBackupCode ? "Backup code" : "Code"}
                </label>
                <input
                  ref={totpInputRef}
                  id="totp-code"
                  name="totp-code"
                  inputMode={useBackupCode ? "text" : "numeric"}
                  autoComplete={useBackupCode ? "off" : "one-time-code"}
                  value={totpCode}
                  onChange={(event) => setTotpCode(event.target.value)}
                  maxLength={useBackupCode ? 8 : 6}
                  aria-describedby={totpError ? totpErrorId : undefined}
                  aria-invalid={Boolean(totpError)}
                  style={inputStyle}
                />
              </div>

              <button
                type="button"
                onClick={() => setUseBackupCode((current) => !current)}
                style={{
                  border: 0,
                  background: "transparent",
                  color: "#0f766e",
                  fontSize: "14px",
                  fontWeight: 600,
                  padding: 0,
                  justifySelf: "start",
                  cursor: "pointer",
                }}
              >
                {useBackupCode ? "Use authenticator code instead" : "Use a backup code instead"}
              </button>
            </div>
          )}

          <div
            id={step === "totp" ? totpErrorId : errorId}
            role="status"
            aria-live="polite"
            aria-atomic="true"
            style={{ minHeight: "1.25em" }}
          >
            {error ? (
              <p style={{ margin: 0, color: "#b91c1c", fontSize: "14px" }}>{error}</p>
            ) : totpError ? (
              <p style={{ margin: 0, color: "#b91c1c", fontSize: "14px" }}>{totpError}</p>
            ) : null}
          </div>

          <button
            type="submit"
            disabled={isSubmitting}
            style={{
              border: 0,
              borderRadius: "14px",
              background: "#0f766e",
              color: "#fff",
              padding: "13px 16px",
              fontSize: "15px",
              fontWeight: 700,
              cursor: isSubmitting ? "progress" : "pointer",
            }}
          >
            {isSubmitting ? (step === "totp" ? "Verifying" : t("auth.signing_in")) : step === "totp" ? "Verify" : t("auth.sign_in")}
          </button>
        </form>

        {providersQuery.data?.google || providersQuery.data?.github ? (
          <div style={{ marginTop: "18px", display: "grid", gap: "10px" }}>
            <div style={{ display: "grid", gap: "10px" }}>
              {providersQuery.data?.google ? (
                <a
                  href="/api/v1/auth/oauth/google"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: "14px",
                    border: "1px solid #d4d4d8",
                    background: "#fff",
                    color: "#18181b",
                    padding: "12px 16px",
                    fontSize: "15px",
                    fontWeight: 600,
                    textDecoration: "none",
                  }}
                  >
                  {t("auth.sign_in_with_google")}
                </a>
              ) : null}
              {providersQuery.data?.github ? (
                <a
                  href="/api/v1/auth/oauth/github"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    borderRadius: "14px",
                    border: "1px solid #d4d4d8",
                    background: "#18181b",
                    color: "#fff",
                    padding: "12px 16px",
                    fontSize: "15px",
                    fontWeight: 600,
                    textDecoration: "none",
                  }}
                  >
                  {t("auth.sign_in_with_github")}
                </a>
              ) : null}
            </div>
          </div>
        ) : null}

        <p style={{ margin: "20px 0 0", color: "#71717a", fontSize: "14px", textAlign: "center" }}>
          {t("auth.first_admin_prompt")}{" "}
          <Link to="/register" style={{ color: "#0f766e", fontWeight: 600, textDecoration: "none" }}>
            {t("auth.register_here")}
          </Link>
        </p>
      </section>
    </main>
  );
}
