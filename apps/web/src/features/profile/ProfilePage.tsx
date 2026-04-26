/**
 * ProfilePage — user profile settings.
 *
 * Route: /profile
 *
 * Sections:
 *   - Current user info card: username, email, and TOTP status display.
 *   - Two-factor authentication card:
 *       Disabled state — "Enable 2FA" button triggers POST /api/v1/auth/totp/setup,
 *         which returns a `secret_base32` and `otpauth_uri`.  The URI is
 *         rendered as a QRCode (qrcode.react) alongside the manual entry code.
 *       Confirm state — user enters their 6-digit code; POST /api/v1/auth/totp/confirm
 *         validates it and returns backup codes shown once in a grid with
 *         per-code clipboard copy buttons.
 *       Enabled state — "Disable 2FA" opens a modal requiring the current
 *         password, then calls POST /api/v1/auth/totp/disable.
 *
 * ProfileSidebar provides the left navigation shared across profile sub-pages.
 *
 * API calls:
 *   GET  /api/v1/users/me
 *   POST /api/v1/auth/totp/setup
 *   POST /api/v1/auth/totp/confirm
 *   POST /api/v1/auth/totp/disable
 */
import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { QRCodeSVG } from "qrcode.react";
import { apiClient } from "../../lib/api-client";
import { ProfileSidebar } from "./ProfileSidebar";

/**
 * ProfilePage renders the user settings page with TOTP setup, confirmation,
 * backup codes, and disable flows.
 */
export function ProfilePage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [setupData, setSetupData] = useState<{ secret_base32: string; otpauth_uri: string } | null>(
    null,
  );
  const [setupCode, setSetupCode] = useState("");
  const [backupCodes, setBackupCodes] = useState<string[] | null>(null);
  const [disableOpen, setDisableOpen] = useState(false);
  const [disablePassword, setDisablePassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const meQuery = useQuery({
    queryKey: ["me"],
    queryFn: () => apiClient.me(),
  });

  const me = meQuery.data;
  const totpEnabled = me?.totp_enabled ?? false;

  async function refreshMe() {
    await queryClient.invalidateQueries({ queryKey: ["me"] });
  }

  async function startSetup() {
    setBusy(true);
    setError(null);
    try {
      const data = await apiClient.setupTotp();
      setSetupData(data);
      setSetupCode("");
      setBackupCodes(null);
    } catch {
      setError("Unable to start 2FA setup.");
    } finally {
      setBusy(false);
    }
  }

  async function confirmSetup() {
    if (!setupData) {
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const result = await apiClient.confirmTotp(setupCode.trim());
      setBackupCodes(result.backup_codes);
      await refreshMe();
    } catch {
      setError("Invalid code.");
    } finally {
      setBusy(false);
    }
  }

  async function disableTotp() {
    setBusy(true);
    setError(null);
    try {
      await apiClient.disableTotp(disablePassword);
      setDisableOpen(false);
      setDisablePassword("");
      setSetupData(null);
      setBackupCodes(null);
      await refreshMe();
    } catch {
      setError("Unable to disable 2FA.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="mx-auto flex w-full max-w-6xl flex-col gap-6 lg:flex-row">
      <ProfileSidebar active="profile" />

      <main className="min-w-0 flex-1">
        <div className="flex flex-col gap-6">
          <header>
            <p className="text-sm uppercase tracking-[0.2em] text-teal-300">{t("nav.profile")}</p>
            <h2 className="mt-2 text-3xl font-semibold text-zinc-50">{t("profile.page_title")}</h2>
          </header>

          <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
            <h3 className="text-lg font-semibold text-zinc-50">{t("profile.current_user")}</h3>
            {me ? (
              <div className="mt-3 space-y-1 text-sm text-zinc-300">
                <p className="font-medium text-zinc-50">{me.username}</p>
                <p>{me.email}</p>
                <p className="text-xs uppercase tracking-[0.18em] text-zinc-500">
                  {totpEnabled ? "Two-factor authentication enabled" : "Two-factor authentication disabled"}
                </p>
              </div>
            ) : (
              <p className="mt-3 text-sm text-zinc-400">{t("profile.loading_user")}</p>
            )}
          </section>

          <section className="rounded-2xl border border-zinc-800 bg-zinc-900/70 p-5">
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3 className="text-lg font-semibold text-zinc-50">Two-factor authentication</h3>
                <p className="mt-1 text-sm text-zinc-400">
                  Protect your account with a time-based one-time password.
                </p>
              </div>

              {totpEnabled ? (
                <span className="rounded-full border border-emerald-500/30 bg-emerald-500/10 px-3 py-1 text-xs font-semibold text-emerald-300">
                  Enabled
                </span>
              ) : (
                <button
                  type="button"
                  onClick={() => {
                    void startSetup();
                  }}
                  disabled={busy}
                  className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
                >
                  Enable two-factor authentication
                </button>
              )}
            </div>

            {error ? <p className="mt-4 text-sm text-red-300">{error}</p> : null}

            {backupCodes ? (
              <div className="mt-5 rounded-2xl border border-amber-500/30 bg-amber-500/10 p-4">
                <p className="font-semibold text-amber-100">
                  Save these backup codes. They will not be shown again.
                </p>
                <div className="mt-4 grid gap-2 sm:grid-cols-2">
                  {backupCodes.map((code) => (
                    <div
                      key={code}
                      className="flex items-center justify-between gap-3 rounded-xl border border-zinc-700 bg-zinc-950 px-3 py-2"
                    >
                      <span className="font-mono text-sm tracking-[0.2em] text-zinc-100">{code}</span>
                      <button
                        type="button"
                        onClick={() => {
                          void navigator.clipboard?.writeText(code);
                        }}
                        className="rounded-md border border-zinc-700 px-2 py-1 text-xs text-zinc-200"
                      >
                        Copy
                      </button>
                    </div>
                  ))}
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setSetupData(null);
                    setBackupCodes(null);
                  }}
                  className="mt-4 rounded-lg border border-zinc-700 px-4 py-2 text-sm text-zinc-100"
                >
                  Done
                </button>
              </div>
            ) : setupData ? (
              <div className="mt-5 grid gap-5 lg:grid-cols-[220px_1fr]">
                <div className="rounded-2xl bg-white p-4">
                  <QRCodeSVG value={setupData.otpauth_uri} size={192} />
                </div>
                <div className="space-y-4">
                  <div className="rounded-2xl border border-zinc-700 bg-zinc-950 p-4">
                    <p className="text-xs uppercase tracking-[0.18em] text-zinc-500">Manual entry code</p>
                    <p className="mt-2 break-all font-mono text-sm text-zinc-100">
                      {setupData.secret_base32}
                    </p>
                  </div>

                  <div>
                    <label className="mb-2 block text-sm text-zinc-300" htmlFor="totp-confirm-code">
                      Confirmation code
                    </label>
                    <input
                      id="totp-confirm-code"
                      value={setupCode}
                      onChange={(event) => setSetupCode(event.target.value)}
                      inputMode="numeric"
                      autoComplete="one-time-code"
                      className="w-full rounded-lg border border-zinc-700 bg-zinc-950 px-3 py-2 text-sm text-zinc-100 outline-none"
                    />
                  </div>

                  <button
                    type="button"
                    onClick={() => {
                      void confirmSetup();
                    }}
                    disabled={busy}
                    className="rounded-lg bg-teal-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
                  >
                    Confirm
                  </button>
                </div>
              </div>
            ) : null}

            {totpEnabled ? (
              <div className="mt-5">
                <button
                  type="button"
                  onClick={() => setDisableOpen(true)}
                  className="rounded-lg border border-red-900 px-4 py-2 text-sm text-red-300"
                >
                  Disable 2FA
                </button>
              </div>
            ) : null}
          </section>
        </div>
      </main>

      {disableOpen ? (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-zinc-950/80 p-4">
          <div className="w-full max-w-md rounded-2xl border border-zinc-700 bg-zinc-950 p-5 shadow-2xl">
            <h3 className="text-xl font-semibold text-zinc-50">Disable two-factor authentication</h3>
            <p className="mt-2 text-sm text-zinc-400">
              Enter your current password to confirm this change.
            </p>
            <input
              type="password"
              value={disablePassword}
              onChange={(event) => setDisablePassword(event.target.value)}
              className="mt-4 w-full rounded-lg border border-zinc-700 bg-zinc-900 px-3 py-2 text-sm text-zinc-100"
              placeholder={t("auth.password")}
            />
            <div className="mt-5 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => {
                  setDisableOpen(false);
                  setDisablePassword("");
                }}
                className="rounded-lg border border-zinc-700 px-4 py-2 text-sm text-zinc-200"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  void disableTotp();
                }}
                disabled={busy}
                className="rounded-lg bg-red-500 px-4 py-2 text-sm font-semibold text-zinc-950 disabled:opacity-60"
              >
                Disable
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
