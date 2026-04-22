import React from "react";
import ReactDOM from "react-dom/client";
import { RouterProvider } from "@tanstack/react-router";
import { router } from "./router";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/query-client";
import i18n, { initializeI18n } from "./i18n";
import { I18nextProvider } from "react-i18next";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

void initializeI18n().then(() => {
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
