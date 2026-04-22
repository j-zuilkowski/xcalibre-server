import { ApiClient } from "@autolibre/shared";
import { useAuthStore } from "./auth-store";

export const apiClient = new ApiClient(
  "",
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
