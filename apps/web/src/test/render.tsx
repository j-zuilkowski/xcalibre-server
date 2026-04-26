import { QueryClientProvider } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";
import {
  RouterProvider,
  createMemoryHistory,
  createRouter,
} from "@tanstack/react-router";
import { I18nextProvider } from "react-i18next";
import { render, type RenderResult } from "@testing-library/react";
import { routeTree } from "../router";
import i18n from "../i18n";
import { useAuthStore } from "../lib/auth-store";
import { makeAdminUser, makeUser } from "./fixtures";
import { makeTestQueryClient } from "./query-client";

type RenderOptions = {
  initialPath?: string;
  authenticated?: boolean;
  user?: ReturnType<typeof makeUser>;
  queryClient?: QueryClient;
};

function makeRouter(pathname: string, tree = routeTree) {
  const history = createMemoryHistory({
    initialEntries: [pathname],
  });

  return createRouter({
    routeTree: tree,
    history,
  });
}

export function renderWithProviders(
  ui: React.ReactElement,
  { initialPath = "/library", authenticated = true, user = makeAdminUser(), queryClient }: RenderOptions = {},
): RenderResult & { router: ReturnType<typeof makeRouter>; queryClient: QueryClient } {
  window.history.pushState({}, "", initialPath);

  if (authenticated) {
    useAuthStore.setState({
      access_token: "test-token",
      refresh_token: "test-refresh",
      user,
    });
  } else {
    useAuthStore.getState().clearAuth();
  }

  const client =
    queryClient ??
    makeTestQueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          gcTime: Infinity,
        },
        mutations: {
          retry: 0,
        },
      },
    });

  const router = makeRouter(initialPath);

  const result = render(
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </I18nextProvider>,
  );

  return Object.assign(result, { router, queryClient: client });
}
