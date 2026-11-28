import { useSignInWithUsernamePassword, useSignUpWithUsernamePassword } from "@convex-dev/auth/methods/username-password/react";
import { api } from "@magi/backend/api";
import { useMutation } from "convex/react";
import { useState } from "react";

const MESSAGES: Record<string, string> = {
  USER_NOT_FOUND: "No account has that username.",
  INVALID_CREDENTIALS: "That password isn't right.",
  RATE_LIMITED: "Too many tries. Wait a minute and try again.",
  USERNAME_TAKEN: "Someone already has that username.",
};

function message(error: string) {
  if (MESSAGES[error]) return MESSAGES[error];
  if (error.startsWith("PASSWORD")) return "Use a longer password, at least 8 characters.";
  if (error.startsWith("USERNAME")) return "Usernames use letters, numbers, dots, dashes and underscores.";
  return "Something went wrong. Check your connection and try again.";
}

export function SignIn() {
  const { signIn, pending: signingIn } = useSignInWithUsernamePassword(api.auth);
  const { signUp, pending: signingUp } = useSignUpWithUsernamePassword(api.auth);
  const setName = useMutation(api.users.setName);
  const [mode, setMode] = useState<"in" | "up">("in");
  const [username, setUsername] = useState("");
  const [name, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState("");
  const pending = signingIn || signingUp;

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError("");
    const credentials = { username: username.trim(), password };
    const result = mode === "in" ? await signIn(credentials) : await signUp(credentials);
    if (result.status !== "complete") {
      setError(message(result.userError.error));
      return;
    }
    if (mode === "up") await setName({ name: name.trim() || credentials.username }).catch(() => {});
  }

  return (
    <form className="card w-full max-w-sm space-y-3" onSubmit={submit}>
      <h1 className="text-lg font-semibold">{mode === "in" ? "Sign in" : "Create an account"}</h1>
      <input className="w-full" autoComplete="username" placeholder="Username" value={username} onChange={(e) => setUsername(e.target.value)} required />
      {mode === "up" && <input className="w-full" autoComplete="name" placeholder="Your name, as the team sees it" value={name} onChange={(e) => setDisplayName(e.target.value)} />}
      <input className="w-full" type="password" autoComplete={mode === "in" ? "current-password" : "new-password"} placeholder="Password" value={password} onChange={(e) => setPassword(e.target.value)} required />
      {error && <p className="text-sm text-red-400" role="alert">{error}</p>}
      <button className="btn-primary w-full" disabled={pending}>{mode === "in" ? "Sign in" : "Create account"}</button>
      <button type="button" className="w-full text-sm text-muted hover:text-fg" onClick={() => { setMode(mode === "in" ? "up" : "in"); setError(""); }}>
        {mode === "in" ? "New to the team? Create an account" : "Have an account? Sign in"}
      </button>
      {mode === "up" && <p className="text-xs text-muted">The first account becomes the admin. Everyone after starts as a singer until an admin gives them a role.</p>}
    </form>
  );
}
