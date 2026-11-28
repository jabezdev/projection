/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as auth from "../auth.js";
import type * as devices from "../devices.js";
import type * as lib_auth from "../lib/auth.js";
import type * as lib_clock from "../lib/clock.js";
import type * as lib_records from "../lib/records.js";
import type * as lib_sequence from "../lib/sequence.js";
import type * as lib_util from "../lib/util.js";
import type * as live from "../live.js";
import type * as serviceParts from "../serviceParts.js";
import type * as services from "../services.js";
import type * as songs from "../songs.js";
import type * as sync from "../sync.js";
import type * as templates from "../templates.js";
import type * as users from "../users.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  auth: typeof auth;
  devices: typeof devices;
  "lib/auth": typeof lib_auth;
  "lib/clock": typeof lib_clock;
  "lib/records": typeof lib_records;
  "lib/sequence": typeof lib_sequence;
  "lib/util": typeof lib_util;
  live: typeof live;
  serviceParts: typeof serviceParts;
  services: typeof services;
  songs: typeof songs;
  sync: typeof sync;
  templates: typeof templates;
  users: typeof users;
}>;

/**
 * A utility for referencing Convex functions in your app's public API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = api.myModule.myFunction;
 * ```
 */
export declare const api: FilterApi<
  typeof fullApi,
  FunctionReference<any, "public">
>;

/**
 * A utility for referencing Convex functions in your app's internal API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = internal.myModule.myFunction;
 * ```
 */
export declare const internal: FilterApi<
  typeof fullApi,
  FunctionReference<any, "internal">
>;

export declare const components: {
  auth: import("@convex-dev/auth/_generated/component.js").ComponentApi<"auth">;
  authPassword: import("@convex-dev/auth/components/password/_generated/component.js").ComponentApi<"authPassword">;
  authUsername: import("@convex-dev/auth/components/username/_generated/component.js").ComponentApi<"authUsername">;
};
