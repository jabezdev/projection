import type { api } from "@magi/backend/api";
import type { FunctionReturnType } from "convex/server";
import { createContext, useContext } from "react";

export type Me = NonNullable<FunctionReturnType<typeof api.users.me>>;

export const MeContext = createContext<Me | null>(null);

/** The signed-in user. Only use below the auth gate in App. */
export function useMe(): Me {
  const me = useContext(MeContext);
  if (!me) throw new Error("useMe used outside the signed-in app");
  return me;
}

export function formatDate(iso: string): string {
  return new Date(`${iso}T00:00:00`).toLocaleDateString(undefined, {
    weekday: "short",
    month: "short",
    day: "numeric",
  });
}

export function todayIso(): string {
  const d = new Date();
  return toIso(d);
}

/** The coming Sunday (today if it's Sunday). */
export function nextSundayIso(): string {
  const d = new Date();
  d.setDate(d.getDate() + ((7 - d.getDay()) % 7));
  return toIso(d);
}

function toIso(d: Date): string {
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}
