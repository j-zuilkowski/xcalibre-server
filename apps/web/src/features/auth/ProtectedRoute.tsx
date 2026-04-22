import { useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { apiClient } from "../../lib/api-client";
import { useAuthStore } from "../../lib/auth-store";
import { AppShell } from "../../components/AppShell";

type GateState = "checking" | "ready" | "redirecting";

export function ProtectedRoute() {
  const navigate = useNavigate();
  const { t } = useTranslation();
  const accessToken = useAuthStore((state) => state.access_token);
  const refreshToken = useAuthStore((state) => state.refresh_token);
  const setAuth = useAuthStore((state) => state.setAuth);
  const clearAuth = useAuthStore((state) => state.clearAuth);
  const [gateState, setGateState] = useState<GateState>("checking");

  useEffect(() => {
    let cancelled = false;

    async function ensureAuthenticated() {
      if (accessToken) {
        if (!cancelled) {
          setGateState("ready");
        }
        return;
      }

      if (!refreshToken) {
        clearAuth();
        if (!cancelled) {
          setGateState("redirecting");
          void navigate({ to: "/login", replace: true });
        }
        return;
      }

      try {
        const refreshed = await apiClient.refresh(refreshToken);
        const currentUser = useAuthStore.getState().user;
        const user = currentUser ?? (await apiClient.me());

        if (!cancelled) {
          setAuth({
            access_token: refreshed.access_token,
            refresh_token: refreshed.refresh_token,
            user,
          });
          setGateState("ready");
        }
      } catch {
        clearAuth();
        if (!cancelled) {
          setGateState("redirecting");
          void navigate({ to: "/login", replace: true });
        }
      }
    }

    if (gateState === "checking") {
      void ensureAuthenticated();
    }

    return () => {
      cancelled = true;
    };
  }, [accessToken, clearAuth, gateState, navigate, refreshToken, setAuth]);

  if (gateState !== "ready") {
    return (
      <main
        style={{
          minHeight: "100vh",
          display: "grid",
          placeItems: "center",
          background: "#fafafa",
          color: "#52525b",
          fontFamily: "Inter, system-ui, sans-serif",
        }}
      >
        <div>{t("auth.checking_session")}</div>
      </main>
    );
  }

  return <AppShell />;
}
