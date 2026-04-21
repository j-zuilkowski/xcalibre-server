import React, { useEffect, useState } from "react";
import {
  ActivityIndicator,
  Pressable,
  StyleSheet,
  Text,
  TextInput,
  View,
} from "react-native";
import { useRouter } from "expo-router";
import type { ApiError } from "@calibre/shared";
import { useApi, getApiBaseUrl, setApiBaseUrl } from "../lib/api";
import { saveTokens } from "../lib/auth";

function toErrorMessage(error: unknown): string {
  const apiError = error as ApiError;
  if (apiError?.status === 401) {
    return "Invalid email or password.";
  }

  return "Unable to sign in right now.";
}

export default function LoginScreen() {
  const client = useApi();
  const router = useRouter();

  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [baseUrl, setBaseUrl] = useState("http://localhost:8080");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [focusedField, setFocusedField] = useState<"email" | "password" | "baseUrl" | null>(null);

  useEffect(() => {
    void (async () => {
      const storedBaseUrl = await getApiBaseUrl();
      setBaseUrl(storedBaseUrl);
    })();
  }, []);

  const signIn = async () => {
    setLoading(true);
    setError(null);

    try {
      await setApiBaseUrl(baseUrl);
      const response = await client.login({
        username: email.trim(),
        password,
      });
      await saveTokens(response.access_token, response.refresh_token);
      router.replace("/(tabs)/library");
    } catch (caught) {
      setError(toErrorMessage(caught));
    } finally {
      setLoading(false);
    }
  };

  return (
    <View style={styles.container}>
      <View style={styles.card}>
        <Text style={styles.title}>Sign In</Text>

        <TextInput
          testID="login-base-url"
          value={baseUrl}
          onChangeText={setBaseUrl}
          onFocus={() => setFocusedField("baseUrl")}
          onBlur={() => setFocusedField(null)}
          placeholder="Server URL"
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
          placeholder="Email"
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
          placeholder="Password"
          placeholderTextColor="#71717a"
          secureTextEntry
          style={[
            styles.input,
            focusedField === "password" ? styles.inputFocused : null,
          ]}
        />

        <Pressable
          testID="login-submit"
          onPress={() => {
            void signIn();
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
            <Text style={styles.signInButtonText}>Sign In</Text>
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
