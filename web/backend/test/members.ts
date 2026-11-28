import authTest from "@convex-dev/auth/test";
import { convexTest } from "convex-test";
import { components } from "../convex/_generated/api";
import type { Doc } from "../convex/_generated/dataModel";
import schema from "../convex/schema";

const modules = import.meta.glob("../convex/**/*.ts");

export function backend() {
  const t = convexTest(schema, modules);
  authTest.registerCore(t);
  return t;
}

export type Backend = ReturnType<typeof backend>;

export async function member(t: Backend, name: string, role: Doc<"users">["role"]) {
  const userId = await t.run((ctx) => ctx.db.insert("users", { name, role }));
  const tokens = await t.run((ctx) =>
    ctx.runMutation(components.auth.public._internal_dangerous_signIn, { claims: { method: "test", profile: {} }, componentPath: "auth", userId, accessTokenTtlSeconds: 3600, refreshInactivityTimeoutSeconds: 86400, sessionMaxLifetimeSeconds: 86400 }),
  );
  return t.withIdentity(authTest.sessionIdentity(tokens));
}
