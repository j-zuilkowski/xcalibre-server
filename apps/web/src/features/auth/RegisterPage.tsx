import { useState, type CSSProperties, type FormEvent } from "react";
import { Link, useNavigate } from "@tanstack/react-router";
import type { ApiError } from "@calibre/shared";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";

function getErrorMessage(error: unknown, fallback: string): string {
  if (typeof error === "object" && error && "status" in error) {
    const apiError = error as ApiError;
    if (apiError.status === 409) {
      return "An account already exists. Please sign in instead.";
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

export function RegisterPage() {
  const navigate = useNavigate();
  const setAuth = useAuthStore((state) => state.setAuth);
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);

    try {
      await apiClient.register({ username, email, password });
      const response = await apiClient.login({ username, password });
      setAuth(response);
      await navigate({ to: "/" });
    } catch (err) {
      setError(getErrorMessage(err, "Unable to create the first account."));
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
        <h1 style={{ margin: "10px 0 8px", fontSize: "34px", lineHeight: 1.1 }}>Create admin</h1>
        <p style={{ margin: "0 0 24px", color: "#52525b", fontSize: "15px" }}>
          Register the first account to initialize the library.
        </p>

        <form onSubmit={handleSubmit} style={{ display: "grid", gap: "18px" }}>
          <div>
            <label htmlFor="register-username" style={labelStyle}>
              Username
            </label>
            <input
              id="register-username"
              name="username"
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              style={inputStyle}
            />
          </div>
          <div>
            <label htmlFor="register-email" style={labelStyle}>
              Email
            </label>
            <input
              id="register-email"
              name="email"
              type="email"
              autoComplete="email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              style={inputStyle}
            />
          </div>
          <div>
            <label htmlFor="register-password" style={labelStyle}>
              Password
            </label>
            <input
              id="register-password"
              name="password"
              type="password"
              autoComplete="new-password"
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
            {isSubmitting ? "Creating account..." : "Create account"}
          </button>
        </form>

        <p style={{ margin: "20px 0 0", color: "#71717a", fontSize: "14px", textAlign: "center" }}>
          Already have an account?{" "}
          <Link to="/login" style={{ color: "#0f766e", fontWeight: 600, textDecoration: "none" }}>
            Sign in
          </Link>
        </p>
      </section>
    </main>
  );
}
