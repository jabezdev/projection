import { defineApp } from "convex/server";
import auth from "@convex-dev/auth/convex.config";
import authPassword from "@convex-dev/auth/components/password/convex.config";
import authUsername from "@convex-dev/auth/components/username/convex.config";

const app = defineApp();
app.use(auth);
app.use(authPassword);
app.use(authUsername);
export default app;
