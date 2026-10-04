// Clerk issues the JWTs. Set CLERK_JWT_ISSUER_DOMAIN in the Convex dashboard
// (Clerk → JWT templates → "convex" → Issuer).
export default {
  providers: [
    {
      domain: process.env.CLERK_JWT_ISSUER_DOMAIN!,
      applicationID: "convex",
    },
  ],
};
