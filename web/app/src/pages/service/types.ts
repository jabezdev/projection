import { api } from "@magi/backend/api";
import type { FunctionReturnType } from "convex/server";

export type Service = NonNullable<FunctionReturnType<typeof api.services.get>>;
export type Item = Service["items"][number];
export type Entry = Item["entries"][number];
