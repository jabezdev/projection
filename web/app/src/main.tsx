import { ClerkProvider, useAuth } from "@clerk/react";
import { ConvexReactClient } from "convex/react";
import { ConvexProviderWithClerk } from "convex/react-clerk";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router";
import { App } from "./App";
import { ErrorProvider } from "./lib/errors";
import "./index.css";

const convexUrl = import.meta.env.VITE_CONVEX_URL as string | undefined;
const clerkKey = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY as string | undefined;

const root = createRoot(document.getElementById("root")!);

if (!convexUrl || !clerkKey) {
  root.render(
    <p style={{ padding: 24 }}>
      Missing VITE_CONVEX_URL or VITE_CLERK_PUBLISHABLE_KEY. Copy web/app/.env.example to
      web/app/.env.local and fill it in.
    </p>,
  );
} else {
  const convex = new ConvexReactClient(convexUrl);
  root.render(
    <StrictMode>
      <ClerkProvider publishableKey={clerkKey}>
        <ConvexProviderWithClerk client={convex} useAuth={useAuth}>
          <BrowserRouter>
            <ErrorProvider>
              <App />
            </ErrorProvider>
          </BrowserRouter>
        </ConvexProviderWithClerk>
      </ClerkProvider>
    </StrictMode>,
  );
}
