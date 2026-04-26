import { ApiClient } from "@xs/shared";
import { useAuthStore } from "./auth-store";

const apiBaseUrl = import.meta.env.VITE_API_URL ?? "";

export const apiClient = new ApiClient(
  apiBaseUrl,
  () => useAuthStore.getState().access_token,
  () => useAuthStore.getState().clearAuth(),
  {
    getRefreshToken: () => useAuthStore.getState().refresh_token,
    onRefreshTokens: (tokens) => {
      const state = useAuthStore.getState();
      if (!state.user) {
        useAuthStore.setState({
          access_token: tokens.access_token,
          refresh_token: tokens.refresh_token,
          user: null,
        });
        return;
      }

      state.setAuth({
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        user: state.user,
      });
    },
  },
);
