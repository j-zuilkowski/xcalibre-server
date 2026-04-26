/**
 * Login screen — handles local credential authentication and TOTP challenges.
 *
 * Route: `/login` (the unauthenticated root; protected routes redirect here when
 * no access token is found in Expo SecureStore).
 *
 * Authentication flow:
 * 1. User enters server URL, username, and password → `POST /api/v1/auth/login`.
 * 2. If the response is a full {@link AuthSession}, tokens are saved to Expo SecureStore
 *    via `saveTokens()` and the app navigates to `/(tabs)/library`.
 * 3. If the response is a {@link LoginTotpRequiredResponse}, the screen transitions
 *    to the TOTP step where the user enters their 6-digit code (or a backup code).
 * 4. TOTP is verified via `POST /api/v1/auth/totp/verify` or `/totp/verify-backup`.
 *
 * SecureStore keys written by `saveTokens()`:
 * - `"access_token"` — short-lived JWT
 * - `"refresh_token"` — long-lived rotation token
 *
 * The server URL is also persisted to SecureStore via `setApiBaseUrl()` and read
 * on mount via `getApiBaseUrl()` so that the field is pre-filled on subsequent
 * launches.
 *
 * Note: OAuth provider buttons (Google/GitHub) are not implemented in the mobile
 * app — OAuth flows require a web browser redirect and are handled in the web app.
 */
import React, { useEffect, useRef, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { useRouter } from "expo-router";
import { useTranslation } from "react-i18next";
import type { ApiError } from "@xs/shared";
import { useApi, getApiBaseUrl, setApiBaseUrl } from "../lib/api";
import { saveTokens } from "../lib/auth";

function toErrorMessage(error: unknown, t: (key: string) => string): string {
  const apiError = error as ApiError;
  if (apiError?.status === 401) {
    return t("auth.invalid_credentials");
  }

  return t("auth.unable_to_sign_in");
}

/**
 * Login screen (Expo Router default export for `/login`).
 *
 * State machine:
 * - `step === "credentials"` — username/password + server URL form
 * - `step === "totp"` — 6-digit TOTP code input (or backup code toggle)
 *
 * The TOTP input is auto-focused when the step transitions to "totp" via a
 * `useEffect` + `totpInputRef`.
 */
export default function LoginScreen() {
  const client = useApi();
  const router = useRouter();
  const { t } = useTranslation();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [baseUrl, setBaseUrl] = useState("http://localhost:8080");
  const [error, setError] = useState<string | null>(null);
  const [totpToken, setTotpToken] = useState("");
  const [totpCode, setTotpCode] = useState("");
  const [useBackupCode, setUseBackupCode] = useState(false);
  const [step, setStep] = useState<"credentials" | "totp">("credentials");
  const [loading, setLoading] = useState(false);
  const [focusedField, setFocusedField] = useState<"email" | "password" | "baseUrl" | null>(null);
  const totpInputRef = useRef<TextInput | null>(null);

  useEffect(() => {
    void (async () => {
      const storedBaseUrl = await getApiBaseUrl();
      setBaseUrl(storedBaseUrl);
    })();
  }, []);

  useEffect(() => {
    if (step === "totp") {
      totpInputRef.current?.focus();
    }
  }, [step, useBackupCode]);

  const signIn = async () => {
    setLoading(true);
    setError(null);

    try {
      await setApiBaseUrl(baseUrl);
      const response = await client.login({
        username: email.trim(),
        password,
      });
      if ("totp_required" in response) {
        setTotpToken(response.totp_token);
        setTotpCode("");
        setUseBackupCode(false);
        setStep("totp");
        return;
      }

      await saveTokens(response.access_token, response.refresh_token);
      router.replace("/(tabs)/library");
    } catch (caught) {
      setError(toErrorMessage(caught, t));
    } finally {
      setLoading(false);
    }
  };

  const verifyTotp = async () => {
    setLoading(true);
    setError(null);

    try {
      const response = useBackupCode
        ? await client.verifyTotpBackup(totpToken, totpCode.trim())
        : await client.verifyTotp(totpToken, totpCode.trim());
      await saveTokens(response.access_token, response.refresh_token);
      router.replace("/(tabs)/library");
    } catch (caught) {
      setError("Invalid code");
    } finally {
      setLoading(false);
    }
  };

  return (
    <View style={styles.container}>
      <View style={styles.card}>
        <Text style={styles.title}>{t("auth.sign_in_title")}</Text>

        {step === "credentials" ? (
          <>
            <TextInput
              testID="login-base-url"
              value={baseUrl}
              onChangeText={setBaseUrl}
              onFocus={() => setFocusedField("baseUrl")}
              onBlur={() => setFocusedField(null)}
              placeholder={t("auth.server_url")}
              placeholderTextColor="#71717a"
              autoCapitalize="none"
              keyboardType="url"
              style={[
                styles.input,
                focusedField === "baseUrl" ? styles.inputFocused : null,
              ]}
            />

            <TextInput
              testID="login-email"
              value={email}
              onChangeText={setEmail}
              onFocus={() => setFocusedField("email")}
              onBlur={() => setFocusedField(null)}
              placeholder={t("auth.email")}
              placeholderTextColor="#71717a"
              autoCapitalize="none"
              keyboardType="email-address"
              style={[styles.input, focusedField === "email" ? styles.inputFocused : null]}
            />

            <TextInput
              testID="login-password"
              value={password}
              onChangeText={setPassword}
              onFocus={() => setFocusedField("password")}
              onBlur={() => setFocusedField(null)}
              placeholder={t("auth.password")}
              placeholderTextColor="#71717a"
              secureTextEntry
              style={[
                styles.input,
                focusedField === "password" ? styles.inputFocused : null,
              ]}
            />
          </>
        ) : (
          <>
            <Text style={styles.totpHeading}>Two-factor authentication</Text>
            <Text style={styles.totpSubtitle}>
              {useBackupCode ? "Enter one of your backup codes." : "Enter the 6-digit code from your authenticator app."}
            </Text>
            <TextInput
              ref={totpInputRef}
              testID="totp-code"
              value={totpCode}
              onChangeText={setTotpCode}
              placeholder={useBackupCode ? "Backup code" : "123456"}
              placeholderTextColor="#71717a"
              keyboardType={useBackupCode ? "default" : "number-pad"}
              autoCapitalize="none"
              style={styles.input}
            />
            <Pressable
              testID="totp-toggle-backup"
              onPress={() => setUseBackupCode((current) => !current)}
            >
              <Text style={styles.backupLink}>
                {useBackupCode ? "Use authenticator code instead" : "Use backup code instead"}
              </Text>
            </Pressable>
          </>
        )}

        <Pressable
          testID="login-submit"
          onPress={() => {
            void (step === "totp" ? verifyTotp() : signIn());
          }}
          disabled={loading}
          style={({ pressed }) => [
            styles.signInButton,
            pressed ? styles.signInButtonPressed : null,
            loading ? styles.signInButtonDisabled : null,
          ]}
        >
          {loading ? (
            <ActivityIndicator color="#ffffff" />
          ) : (
            <Text style={styles.signInButtonText}>
              {step === "totp" ? "Verify" : t("auth.sign_in")}
            </Text>
          )}
        </Pressable>

        {error ? <Text style={styles.errorText}>{error}</Text> : null}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    justifyContent: "center",
    padding: 24,
    backgroundColor: "#fafafa",
  },
  card: {
    borderWidth: 1,
    borderColor: "#e4e4e7",
    borderRadius: 16,
    backgroundColor: "#ffffff",
    padding: 20,
    gap: 12,
  },
  title: {
    fontSize: 24,
    fontWeight: "700",
    color: "#18181b",
    marginBottom: 8,
  },
  input: {
    borderWidth: 1,
    borderColor: "#e4e4e7",
    borderRadius: 10,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
    color: "#18181b",
  },
  inputFocused: {
    borderColor: "#0f766e",
    shadowColor: "#0f766e",
    shadowOffset: { width: 0, height: 0 },
    shadowOpacity: 0.3,
    shadowRadius: 3,
  },
  totpHeading: {
    fontSize: 20,
    fontWeight: "700",
    color: "#18181b",
  },
  totpSubtitle: {
    fontSize: 13,
    color: "#52525b",
  },
  backupLink: {
    color: "#0f766e",
    fontSize: 13,
    fontWeight: "600",
  },
  signInButton: {
    borderRadius: 10,
    paddingVertical: 12,
    alignItems: "center",
    backgroundColor: "#0f766e",
  },
  signInButtonPressed: {
    opacity: 0.9,
  },
  signInButtonDisabled: {
    opacity: 0.7,
  },
  signInButtonText: {
    color: "#ffffff",
    fontWeight: "600",
    fontSize: 16,
  },
  errorText: {
    color: "#dc2626",
    fontSize: 13,
  },
});
