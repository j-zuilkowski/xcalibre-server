import { useState, type CSSProperties, type FormEvent } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import type { ApiError } from "@autolibre/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";

function getErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error && "status" in error) {
    const apiError = error as ApiError;
    if (apiError.status === 401) {
      return "Invalid username or password.";
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

export function LoginPage() {
  const navigate = useNavigate();
  const setAuth = useAuthStore((state) => state.setAuth);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const providersQuery = useQuery({
    queryKey: ["auth-providers"],
    queryFn: () => apiClient.getAuthProviders(),
    staleTime: 5 * 60_000,
  });

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);

    try {
      const response = await apiClient.login({ username, password });
      setAuth(response);
      await navigate({ to: "/" });
    } catch (err) {
      setError(getErrorMessage(err, "Unable to sign in."));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <main style={pageStyle}>
      <section style={cardStyle}>
        <p style={{ margin: 0, color: "#0f766e", fontSize: "13px", fontWeight: 700, letterSpacing: "0.08em", textTransform: "uppercase" }}>
          calibre-web
        </p>
        <h1 style={{ margin: "10px 0 8px", fontSize: "34px", lineHeight: 1.1 }}>Sign in</h1>
        <p style={{ margin: "0 0 24px", color: "#52525b", fontSize: "15px" }}>
          Access your library and pick up where you left off.
        </p>

        <form onSubmit={handleSubmit} style={{ display: "grid", gap: "18px" }}>
          <div>
            <label htmlFor="login-username" style={labelStyle}>
              Username
            </label>
            <input
              id="login-username"
              name="username"
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              style={inputStyle}
            />
          </div>
          <div>
            <label htmlFor="login-password" style={labelStyle}>
              Password
            </label>
            <input
              id="login-password"
              name="password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              style={inputStyle}
            />
          </div>

          {error ? (
            <p style={{ margin: 0, color: "#b91c1c", fontSize: "14px", minHeight: "1.25em" }}>{error}</p>
          ) : (
            <div style={{ minHeight: "1.25em" }} />
          )}

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
            {isSubmitting ? "Signing in..." : "Sign in"}
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
                  Sign in with Google
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
                  Sign in with GitHub
                </a>
              ) : null}
            </div>
          </div>
        ) : null}

        <p style={{ margin: "20px 0 0", color: "#71717a", fontSize: "14px", textAlign: "center" }}>
          Need the first admin account?{" "}
          <Link to="/register" style={{ color: "#0f766e", fontWeight: 600, textDecoration: "none" }}>
            Register here
          </Link>
        </p>
      </section>
    </main>
  );
}
