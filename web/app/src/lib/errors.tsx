import { ConvexError } from "convex/values";
import { createContext, useCallback, useContext, useState, type ReactNode } from "react";

const ErrorContext = createContext<(error: unknown) => void>(() => {});

/** Shows the latest failed action in a dismissible banner. */
export function ErrorProvider({ children }: { children: ReactNode }) {
  const [message, setMessage] = useState<string | null>(null);
  const report = useCallback((error: unknown) => {
    console.error(error);
    setMessage(
      error instanceof ConvexError
        ? String(error.data)
        : "Something went wrong. Check your connection and try again.",
    );
  }, []);

  return (
    <ErrorContext.Provider value={report}>
      {children}
      {message && (
        <div className="fixed inset-x-4 bottom-20 z-50 mx-auto flex max-w-md items-start gap-3 rounded-xl border border-red-500/50 bg-red-950 p-3 text-sm md:bottom-4">
          <span className="flex-1">{message}</span>
          <button className="text-muted hover:text-white" onClick={() => setMessage(null)}>
            ✕
          </button>
        </div>
      )}
    </ErrorContext.Provider>
  );
}

/**
 * Wraps a mutation so a failure shows the banner instead of an unhandled
 * rejection. Returns undefined when it failed.
 */
export function useSafe() {
  const report = useContext(ErrorContext);
  return useCallback(
    <Args extends unknown[], R>(fn: (...args: Args) => Promise<R>) =>
      async (...args: Args): Promise<R | undefined> => {
        try {
          return await fn(...args);
        } catch (error) {
          report(error);
          return undefined;
        }
      },
    [report],
  );
}
