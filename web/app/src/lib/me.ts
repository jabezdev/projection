import type { api } from "@magi/backend/api";
import type { FunctionReturnType } from "convex/server";
import { createContext, useContext } from "react";

export type Me = NonNullable<FunctionReturnType<typeof api.users.me>>;

export const MeContext = createContext<Me | null>(null);

export function useMe(): Me {
  const me = useContext(MeContext);
  if (!me) throw new Error("useMe used outside the signed-in app");
  return me;
}

export function formatWhen(ms: number): string {
  return new Date(ms).toLocaleString(undefined, { weekday: "short", month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
}

export function startOfToday(): number {
  const d = new Date();
  d.setHours(0, 0, 0, 0);
  return d.getTime();
}

export function nextSundayMorning(): number {
  const d = new Date();
  d.setDate(d.getDate() + ((7 - d.getDay()) % 7));
  d.setHours(10, 0, 0, 0);
  return d.getTime();
}

export function toLocalInput(ms: number): string {
  const d = new Date(ms);
  d.setMinutes(d.getMinutes() - d.getTimezoneOffset());
  return d.toISOString().slice(0, 16);
}

export const fromLocalInput = (value: string) => new Date(value).getTime();
