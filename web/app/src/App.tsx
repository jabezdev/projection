import { useAuthActions } from "@convex-dev/auth/react";
import { api } from "@magi/backend/api";
import { Authenticated, AuthLoading, Unauthenticated, useQuery } from "convex/react";
import { CalendarDays, Library as LibraryIcon, LogOut, MonitorPlay, Users } from "lucide-react";
import { Navigate, NavLink, Route, Routes } from "react-router";
import { MeContext } from "./lib/me";
import { Library } from "./pages/Library";
import { NewSong } from "./pages/NewSong";
import { NowShowing } from "./pages/NowShowing";
import { SignIn } from "./pages/SignIn";
import { Templates } from "./pages/Templates";
import { ServicePage } from "./pages/service/ServicePage";
import { Services } from "./pages/Services";
import { SongPage } from "./pages/song/SongPage";
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
  const me = useQuery(api.users.me);
  const { signOut } = useAuthActions();

  if (!me) return <Centered>Setting up your account…</Centered>;

  return (
    <MeContext.Provider value={me}>
      <div className="mx-auto flex min-h-dvh max-w-5xl flex-col pb-20 md:pb-0">
        <header className="sticky top-0 z-10 flex items-center gap-6 border-b border-line bg-bg/90 px-4 py-2.5 backdrop-blur">
          <span className="flex items-center gap-2.5">
            <img src="/icon.svg" alt="" className="size-7" />
            <span className="font-label text-lg font-semibold tracking-[0.14em]">PALTO</span>
            <span className="hidden text-sm text-faint sm:inline">Library</span>
          </span>
          <nav className="hidden gap-1 md:flex">
            <Tabs />
          </nav>
          <span className="ml-auto text-right text-xs leading-tight text-muted">
            {me.name}
            <span className="block text-faint capitalize">{me.role}</span>
          </span>
          <button className="btn-icon" title="Sign out" aria-label="Sign out" onClick={() => void signOut()}>
            <LogOut size={16} />
          </button>
        </header>

        <main className="flex-1 px-4 py-5">
          <Routes>
            <Route path="/" element={<Navigate to="/services" replace />} />
            <Route path="/library" element={<Library />} />
            <Route path="/library/new" element={<NewSong />} />
            <Route path="/library/:songId" element={<SongPage />} />
            <Route path="/services" element={<Services />} />
            <Route path="/services/:serviceId" element={<ServicePage />} />
            <Route path="/templates" element={<Templates />} />
            <Route path="/now" element={<NowShowing />} />
            <Route path="/team" element={<Team />} />
            <Route path="*" element={<Navigate to="/services" replace />} />
          </Routes>
        </main>

        <nav className="fixed inset-x-0 bottom-0 z-10 grid grid-cols-4 border-t border-line bg-bg/95 pb-[env(safe-area-inset-bottom)] backdrop-blur md:hidden">
          <Tabs />
        </nav>
      </div>
    </MeContext.Provider>
  );
}

const TABS = [
  { to: "/services", label: "Services", icon: CalendarDays },
  { to: "/now", label: "Now", icon: MonitorPlay },
  { to: "/library", label: "Songs", icon: LibraryIcon },
  { to: "/team", label: "Team", icon: Users },
];

function Tabs() {
  const tab = ({ isActive }: { isActive: boolean }) =>
    `flex flex-col items-center gap-1 px-3 py-2 text-[11px] font-medium md:flex-row md:gap-2 md:rounded-lg md:text-sm ${
      isActive ? "text-fg md:bg-hover" : "text-faint hover:text-fg md:text-muted"
    }`;
  return (
    <>
      {TABS.map(({ to, label, icon: Icon }) => (
        <NavLink key={to} to={to} className={tab}>
          {({ isActive }) => (
            <>
              <Icon size={18} className={isActive ? "text-accent" : ""} />
              {label}
            </>
          )}
        </NavLink>
      ))}
    </>
  );
}

function Centered({ children }: { children: React.ReactNode }) {
  return <div className="grid min-h-dvh place-items-center p-4 text-muted">{children}</div>;
}
