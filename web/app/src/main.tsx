import { ConvexAuthProvider } from "@convex-dev/auth/react";
import { api } from "@magi/backend/api";
import { ConvexReactClient } from "convex/react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { App } from "./App";
import { ErrorProvider } from "./lib/errors";
import "@fontsource/barlow/latin-400.css";
import "@fontsource/barlow/latin-500.css";
import "@fontsource/barlow/latin-600.css";
import "@fontsource/barlow/latin-700.css";
import "@fontsource/barlow-condensed/latin-600.css";
import "./index.css";

const convexUrl = import.meta.env.VITE_CONVEX_URL as string | undefined;

const root = createRoot(document.getElementById("root")!);

if (!convexUrl) {
  root.render(
    <p style={{ padding: 24 }}>
      Missing VITE_CONVEX_URL. Copy web/app/.env.example to web/app/.env.local and fill it in.
    </p>,
  );
} else {
  const convex = new ConvexReactClient(convexUrl);
  root.render(
    <StrictMode>
      <ConvexAuthProvider client={convex} api={api.auth}>
        <BrowserRouter>
          <ErrorProvider>
            <App />
          </ErrorProvider>
        </BrowserRouter>
      </ConvexAuthProvider>
    </StrictMode>,
  );
}
