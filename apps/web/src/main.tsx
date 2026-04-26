/**
 * Application entry point.
 *
 * Bootstrap order:
 * 1. i18n is initialised asynchronously (loads locale JSON from /locales/).
 *    Errors are silently swallowed so a network failure never blocks the app.
 * 2. The React tree is mounted only after i18n resolves to avoid a flash of
 *    untranslated text.
 * 3. Provider hierarchy: I18nextProvider → QueryClientProvider → RouterProvider.
 *    All server-state fetching goes through TanStack Query; routing through
 *    TanStack Router (file-based routes defined in router.tsx).
 */
import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/query-client";
import i18n, { initializeI18n } from "./i18n";
import { I18nextProvider } from "react-i18next";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

void initializeI18n()
  .catch(() => {})
  .then(() => {
    root.render(
      <React.StrictMode>
        <I18nextProvider i18n={i18n}>
          <QueryClientProvider client={queryClient}>
            <RouterProvider router={router} />
          </QueryClientProvider>
        </I18nextProvider>
      </React.StrictMode>,
    );
  });
