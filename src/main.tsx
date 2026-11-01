import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "@tanstack/react-router";
import { info } from "@tauri-apps/plugin-log";
import React from "react";
import ReactDOM from "react-dom/client";
import { limitContextMenu } from "./contextMenu";
import { reportCspViolations } from "./cspReport";
import "./i18n";
import "./index.css";
import { applyTheme } from "./preferences";
import { router } from "./router";

reportCspViolations();
// Shows in the log that scripts run and reach the app (csp-smoke.yml).
void info("Interface started").catch(() => {});
applyTheme();
// In development the full menu stays, for Inspect.
if (import.meta.env.PROD) limitContextMenu();

const queryClient = new QueryClient();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  </React.StrictMode>,
);
