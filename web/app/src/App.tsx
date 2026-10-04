import { SignIn, UserButton } from "@clerk/react";
import { api } from "@magi/backend/api";
import { Authenticated, AuthLoading, Unauthenticated, useMutation, useQuery } from "convex/react";
import { useEffect } from "react";
import { Navigate, NavLink, Route, Routes } from "react-router";
import { MeContext } from "./lib/me";
import { Library } from "./pages/Library";
import { NewSong } from "./pages/NewSong";
import { ServicePage } from "./pages/ServicePage";
import { Services } from "./pages/Services";
import { SongPage } from "./pages/SongPage";
import { Team } from "./pages/Team";

export function App() {
  return (
    <>
      <AuthLoading>
        <Centered>Loading…</Centered>
      </AuthLoading>
      <Unauthenticated>
        <Centered>
          <SignIn />
        </Centered>
      </Unauthenticated>
      <Authenticated>
        <SignedInApp />
      </Authenticated>
    </>
  );
}

function SignedInApp() {
  const ensure = useMutation(api.users.ensure);
  const me = useQuery(api.users.me);

  useEffect(() => {
    void ensure();
  }, [ensure]);

  if (!me) return <Centered>Setting up your account…</Centered>;

  return (
    <MeContext.Provider value={me}>
      <div className="mx-auto flex min-h-dvh max-w-4xl flex-col pb-20 md:pb-0">
        <header className="sticky top-0 z-10 flex items-center gap-6 border-b border-line/60 bg-bg/90 px-4 py-3 backdrop-blur">
          <span className="font-semibold">Song Library</span>
          <nav className="hidden gap-1 md:flex">
            <Tabs />
          </nav>
          <span className="ml-auto text-xs text-muted capitalize">{me.role}</span>
          <UserButton />
        </header>

        <main className="flex-1 px-4 py-4">
          <Routes>
            <Route path="/" element={<Navigate to="/services" replace />} />
            <Route path="/library" element={<Library />} />
            <Route path="/library/new" element={<NewSong />} />
            <Route path="/library/:songId" element={<SongPage />} />
            <Route path="/services" element={<Services />} />
            <Route path="/services/:serviceId" element={<ServicePage />} />
            <Route path="/team" element={<Team />} />
            <Route path="*" element={<Navigate to="/services" replace />} />
          </Routes>
        </main>

        <nav className="fixed inset-x-0 bottom-0 z-10 flex justify-around border-t border-line/60 bg-bg/95 pb-[env(safe-area-inset-bottom)] backdrop-blur md:hidden">
          <Tabs />
        </nav>
      </div>
    </MeContext.Provider>
  );
}

function Tabs() {
  const tab = ({ isActive }: { isActive: boolean }) =>
    `rounded-lg px-4 py-2.5 text-sm ${isActive ? "text-white md:bg-hover" : "text-muted hover:text-white"}`;
  return (
    <>
      <NavLink to="/services" className={tab}>
        Services
      </NavLink>
      <NavLink to="/library" className={tab}>
        Library
      </NavLink>
      <NavLink to="/team" className={tab}>
        Team
      </NavLink>
    </>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return <div className="grid min-h-dvh place-items-center p-4 text-muted">{children}</div>;
}
