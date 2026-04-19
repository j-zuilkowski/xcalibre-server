import { create } from "zustand";
import type { User } from "@calibre/shared";

type AuthState = {
  access_token: string | null;
  refresh_token: string | null;
  user: User | null;
  setAuth: (auth: {
    access_token: string;
    refresh_token: string;
    user: User;
  }) => void;
  clearAuth: () => void;
};

export const useAuthStore = create<AuthState>((set) => ({
  access_token: null,
  refresh_token: null,
  user: null,
  setAuth: (auth) => set(auth),
  clearAuth: () =>
    set({
      access_token: null,
      refresh_token: null,
      user: null,
    }),
}));
