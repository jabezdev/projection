# Song library (web)

A phone-friendly site (installable as an app) for the worship team. It's where the week's prep happens before the projection app takes over:

| When | Who | Where |
|---|---|---|
| Mon–Tue | Lyric team | **Library** tab: add songs by pasting lyrics, fix typos, set the usual order |
| Wed–Fri | Song leader | **Services** tab: plan the service, pick songs, arrange sections, change lyrics for that service only |
| Sat–Sun | Singers | **Services** tab (lyrics view): read the arrangement |
| Sat–Sun | Operator | Projection app (syncing with the web app is the next step; see [task.md](../task.md)) |

Data lives in [Convex](https://convex.dev) (a hosted database that pushes changes to every open screen instantly). Sign-in is handled by [Clerk](https://clerk.com). See [../ARCHITECTURE.md](../ARCHITECTURE.md) for the data model.

All commands below run from this `web/` folder. Needs [Bun](https://bun.sh/).

## First-time setup

1. Create a Clerk application. In Clerk's dashboard: **JWT templates → New → Convex** (keep the name `convex`) and copy its **Issuer** URL.
2. Start the backend. The first run asks you to log in to Convex and create a project:
   ```bash
   bun install
   bun run dev:backend
   ```
   Then in the Convex dashboard → Settings → Environment variables, set `CLERK_JWT_ISSUER_DOMAIN` to the Issuer URL.
3. Copy `app/.env.example` to `app/.env.local` and fill in `VITE_CONVEX_URL` (printed by step 2, also in `backend/.env.local`) and `VITE_CLERK_PUBLISHABLE_KEY` (Clerk → API keys).
4. In a second terminal: `bun run dev:app`, then open http://localhost:5174.

**The first person to sign in becomes admin.** Everyone after starts as a *singer* (read-only) until an admin gives them a role on the **Team** tab.

## Everyday commands

| What | Command |
|---|---|
| Run the backend (watches and pushes changes) | `bun run dev:backend` |
| Run the web app | `bun run dev:app` |
| Typecheck everything | `bun run typecheck` |
| Run all tests | `bun run test` |
| Build the web app (output in `app/dist`) | `bun run build` |
| Deploy the backend to production | `bun run deploy:backend` |

## Deploying

The backend and the web app deploy separately:

- **Backend:** `bun run deploy:backend` pushes the Convex functions and schema to production.
- **Web app:** point your host (Vercel, Netlify, Cloudflare Pages) at this repo with root directory `web`, build command `bun run build`, output directory `app/dist`. Set `VITE_CONVEX_URL` (production URL) and `VITE_CLERK_PUBLISHABLE_KEY` in the host's settings.

## Folder layout

```
app/        The web app (React PWA)
backend/    Convex: database schema (convex/schema.ts) and server functions
shared/     Lyric parsing and slide splitting, for the web app and later the desktop
```
