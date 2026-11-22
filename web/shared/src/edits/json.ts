import type { Json } from "./types";

export function json(value: unknown): Json {
  if (value === undefined || value === null) return null;
  if (Array.isArray(value)) return value.map(json);
  if (typeof value === "object") {
    const out: { [key: string]: Json } = {};
    for (const [key, field] of Object.entries(value)) if (field !== undefined && field !== null) out[key] = json(field);
    return out;
  }
  return value as Json;
}

export function same(a: unknown, b: unknown): boolean {
  return JSON.stringify(sorted(json(a))) === JSON.stringify(sorted(json(b)));
}

function sorted(value: Json): Json {
  if (Array.isArray(value)) return value.map(sorted);
  if (value && typeof value === "object") return Object.fromEntries(Object.keys(value).sort().map((key) => [key, sorted(value[key])]));
  return value;
}

export function stored<T>(value: Json): T | undefined {
  return value === null ? undefined : (structuredClone(value) as T);
}
