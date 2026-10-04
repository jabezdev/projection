import { invoke, listen, isTauri } from './tauri';
import { useEffect, useMemo, useState } from 'react';

type Transport = 'stopped' | 'playing' | 'paused';

type BackgroundKind = 'image' | 'video';

type Background = {
  kind: BackgroundKind;
  path: string;
};

type ProgramState = {
  mode: 'manual' | 'automated' | 'external';
  transport: Transport;
  time_ms: number;
  active_item_id: number | null;
  next_item_id: number | null;
  active_step_index: number | null;
  active_text: string;
  background: Background | null;
};

type SessionItemKind =
  | { kind: 'text'; data: { steps: string[] } }
  | { kind: 'image'; data: { path: string } }
  | { kind: 'video'; data: { path: string } }
  | { kind: 'presentation'; data: { path: string } };

type SessionItem = {
  id: number;
  title: string | null;
  kind: SessionItemKind;
};

type SessionModel = {
  items: SessionItem[];
};

type ProjectModel = {
  id: number;
  name: string;
  session: SessionModel;
};

type ProjectHeader = {
  id: number;
  name: string;
  updated_at_ms: number;
};

type EngineSnapshot = {
  program_state: ProgramState;
  version: number;
  project: ProjectModel | null;
};

// Each state event carries the complete state of one domain; replace, don't merge.
type EngineEvent =
  | { type: 'project_changed'; payload: { project: ProjectModel | null; version: number } }
  | { type: 'program_changed'; payload: { program_state: ProgramState; version: number } }
  | { type: 'error'; payload: { source: string; message: string; recoverable: boolean } };

const DEMO_TEXT_STEPS = [
  'Amazing Grace, how sweet the sound',
  'That saved a wretch like me',
  'I once was lost, but now am found',
  'Was blind, but now I see',
  "'Twas grace that taught my heart to fear",
  'And grace my fears relieved',
  'How precious did that grace appear',
  'The hour I first believed',
];

function kindLabel(kind: SessionItemKind): string {
  switch (kind.kind) {
    case 'text':
      return 'Text';
    case 'image':
      return 'Image';
    case 'video':
      return 'Video';
    case 'presentation':
      return 'Presentation';
  }
}

function kindDetail(kind: SessionItemKind): string {
  switch (kind.kind) {
    case 'text':
      return `${kind.data.steps.length} steps`;
    case 'image':
    case 'video':
    case 'presentation':
      return kind.data.path;
  }
}

function formatClock(d: Date): string {
  const hh = String(d.getHours()).padStart(2, '0');
  const mm = String(d.getMinutes()).padStart(2, '0');
  const ss = String(d.getSeconds()).padStart(2, '0');
  return `${hh}:${mm}:${ss}`;
}

function getMode(): 'control' | 'confidence' {
  if (typeof window === 'undefined') return 'control';

  const url = new URL(window.location.href);

  // Path-based routing for fast browser workflow.
  const normalizedPath = url.pathname.replace(/\/+$/, '');
  if (normalizedPath === '/confidence') return 'confidence';

  // Hash-based routing fallback (useful for file:// or constrained hosts).
  if (url.hash === '#/confidence' || url.hash.startsWith('#/confidence?')) return 'confidence';

  // Query flag (used by the Tauri confidence window).
  if (url.searchParams.get('mode') === 'confidence') return 'confidence';

  return 'control';
}

function App() {
  const mode = useMemo(() => getMode(), []);

  const [transport, setTransport] = useState<Transport>('stopped');
  const [status, setStatus] = useState('Idle');
  const [isLive, setIsLive] = useState(false);
  const [activeText, setActiveText] = useState('');
  const [programState, setProgramState] = useState<ProgramState | null>(null);

  const [project, setProject] = useState<ProjectModel | null>(null);
  const [projects, setProjects] = useState<ProjectHeader[]>([]);
  const [projectName, setProjectName] = useState('');
  const [clock, setClock] = useState(() => formatClock(new Date()));

  const ipcLabel = isTauri() ? 'Tauri IPC OK' : 'IPC disabled (UI-only)';

  const applyProgramState = (ps: ProgramState) => {
    setProgramState(ps);
    setTransport(ps.transport);
    setActiveText(ps.active_text || '');
    setIsLive(ps.active_text.trim().length > 0);
    setStatus(ps.transport === 'playing' ? 'Playing' : ps.transport === 'paused' ? 'Paused' : 'Stopped');
  };

  const activeItem =
    project && programState?.active_item_id != null
      ? project.session.items.find((it) => it.id === programState.active_item_id) || null
      : null;
  const nextItem =
    project && programState?.next_item_id != null
      ? project.session.items.find((it) => it.id === programState.next_item_id) || null
      : null;

  useEffect(() => {
    const t = setInterval(() => setClock(formatClock(new Date())), 250);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    let unlisten: undefined | (() => void);

    (async () => {
      try {
        const snapshot = await invoke<EngineSnapshot>('get_engine_snapshot');
        applyProgramState(snapshot.program_state);
        setProject(snapshot.project);
      } catch {
        // Backend might not be ready yet.
      }

      try {
        const list = await invoke<ProjectHeader[]>('list_projects');
        if (Array.isArray(list)) {
          setProjects(list);
          if (!project && list.length > 0) {
            await invoke('load_project', { project_id: list[0].id });
          }
        }
      } catch {
        // UI-only mode.
      }

      unlisten = await listen<EngineEvent>('engine:event', (evt) => {
        const payload = evt.payload;
        if (payload.type === 'project_changed') {
          setProject(payload.payload.project);
        } else if (payload.type === 'program_changed') {
          applyProgramState(payload.payload.program_state);
        } else if (payload.type === 'error') {
          console.warn(`[engine:${payload.payload.source}]`, payload.payload.message);
        }
      });
    })();

    return () => {
      unlisten?.();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const togglePlay = async () => {
    if (transport === 'playing') {
      await invoke('pause');
    } else {
      await invoke('play');
    }
  };

  const handleNext = async () => {
    await invoke('nav_next');
  };

  const handlePrev = async () => {
    await invoke('nav_prev');
  };

  const createDemoProject = async () => {
    const created = await invoke<ProjectModel>('create_project', { name: 'Demo Session' });
    const baseId = Date.now();
    const demo: ProjectModel = {
      ...created,
      session: {
        items: [
          {
            id: baseId + 1,
            title: 'Welcome (Text)',
            kind: { kind: 'text', data: { steps: DEMO_TEXT_STEPS } },
          },
          {
            id: baseId + 2,
            title: 'Background (Image)',
            kind: { kind: 'image', data: { path: 'assets/bg1.png' } },
          },
          {
            id: baseId + 3,
            title: 'Motion (Video)',
            kind: { kind: 'video', data: { path: 'assets/video1.mp4' } },
          },
        ],
      },
    };
    await invoke('save_project', { project: demo });
    const list = await invoke<ProjectHeader[]>('list_projects');
    if (Array.isArray(list)) setProjects(list);
    await invoke('load_project', { project_id: demo.id });
  };

  if (mode === 'confidence') {
    return (
      <div className="min-h-screen bg-bg-deep text-zinc-100 flex flex-col font-sans selection:bg-accent-primary/30">
        <header className="h-14 glass sticky top-0 z-50 flex items-center justify-between px-6 border-b border-white/5">
          <div className="flex items-center gap-3">
            <div className="w-7 h-7 rounded-lg bg-gradient-to-br from-accent-primary to-accent-secondary flex items-center justify-center shadow-lg shadow-accent-primary/20">
              <span className="text-white font-bold text-[10px]">M</span>
            </div>
            <div className="text-xs font-black uppercase tracking-widest text-zinc-400">Confidence</div>
          </div>
          <div className="text-sm font-mono text-white/80">{clock}</div>
        </header>

        <main className="flex-1 grid grid-cols-12 gap-6 p-6 max-w-[1400px] mx-auto w-full">
          <section className="col-span-8 bg-bg-card rounded-2xl border border-white/5 p-6 shadow-2xl">
            <div className="flex items-center justify-between">
              <h2 className="text-[10px] font-black uppercase tracking-widest text-zinc-500">Now</h2>
              <div className="text-[10px] text-zinc-600 font-mono">{ipcLabel}</div>
            </div>
            <div className="mt-4">
              <div className="text-sm text-zinc-400 font-semibold">
                {activeItem ? `${activeItem.title || kindLabel(activeItem.kind)}` : 'No item selected'}
              </div>
              <div className="mt-3 text-3xl leading-tight font-medium text-white drop-shadow-lg">
                {activeText || '—'}
              </div>
            </div>
          </section>

          <aside className="col-span-4 bg-bg-card rounded-2xl border border-white/5 p-6 shadow-2xl">
            <h2 className="text-[10px] font-black uppercase tracking-widest text-zinc-500">Next</h2>
            <div className="mt-4 text-lg font-semibold text-white/90">
              {nextItem ? `${nextItem.title || kindLabel(nextItem.kind)}` : '—'}
            </div>
            <div className="mt-6">
              <div
                className={`inline-flex items-center gap-2 px-3 py-1 rounded-full text-xs font-medium border transition-colors ${
                  isLive
                    ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                    : 'bg-zinc-800 text-zinc-500 border-zinc-700'
                }`}
              >
                <div className={`w-1.5 h-1.5 rounded-full ${isLive ? 'bg-emerald-400' : 'bg-zinc-600'}`} />
                {isLive ? 'LIVE OUTPUT' : 'STANDBY'}
              </div>
              <div className="mt-2 text-xs text-zinc-500 font-mono">{status}</div>
            </div>
          </aside>
        </main>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-bg-deep text-zinc-100 flex flex-col font-sans selection:bg-accent-primary/30">
      <header className="h-16 glass sticky top-0 z-50 flex items-center justify-between px-8 border-b border-white/5">
        <div className="flex items-center gap-3">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-accent-primary to-accent-secondary flex items-center justify-center shadow-lg shadow-accent-primary/20">
            <span className="text-white font-bold text-xs">M</span>
          </div>
          <h1 className="text-lg font-semibold tracking-tight text-white uppercase tracking-widest">Magi Projection</h1>
        </div>

        <div className="flex items-center gap-4">
          <div
            className={`flex items-center gap-2 px-3 py-1 rounded-full text-xs font-medium border transition-colors ${
              isLive
                ? 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20'
                : 'bg-zinc-800 text-zinc-500 border-zinc-700'
            }`}
          >
            <div className={`w-1.5 h-1.5 rounded-full ${isLive ? 'bg-emerald-400 animate-pulse' : 'bg-zinc-600'}`} />
            {isLive ? 'LIVE OUTPUT' : 'STANDBY'}
          </div>
          <span className="text-xs text-zinc-500 font-mono">{status}</span>
        </div>
      </header>

      <main className="flex-1 grid grid-cols-12 gap-6 p-8 max-w-[1600px] mx-auto w-full">
        <section className="col-span-8 flex flex-col gap-4 animate-in">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-bold text-zinc-500 uppercase tracking-widest">Session Playlist</h2>
            <div className="text-[10px] text-zinc-600 font-mono">{project ? project.name : 'No Project Loaded'}</div>
          </div>

          <div className="bg-bg-card rounded-2xl border border-white/5 overflow-hidden shadow-2xl flex-1 flex flex-col">
            <div className="p-2 space-y-1 overflow-y-auto custom-scrollbar">
              {project?.session.items?.length ? (
                project.session.items.map((item, index) => {
                  const isActive = programState?.active_item_id === item.id;
                  return (
                    <button
                      key={item.id}
                      onClick={() => invoke('go_to_item', { item_id: item.id })}
                      className={`w-full text-left px-6 py-5 rounded-xl transition-all duration-300 group flex items-center gap-6 relative ${
                        isActive
                          ? 'bg-accent-primary/10 text-white border border-accent-primary/20 shadow-lg'
                          : 'hover:bg-bg-hover text-zinc-400 border border-transparent'
                      }`}
                    >
                      {isActive && (
                        <div className="absolute left-0 top-1/2 -translate-y-1/2 w-1 h-8 bg-accent-primary rounded-r-full" />
                      )}
                      <span className={`text-xs font-mono w-6 shrink-0 ${isActive ? 'text-accent-primary' : 'text-zinc-700'}`}>
                        {String(index + 1).padStart(2, '0')}
                      </span>
                      <div className="flex flex-col gap-1">
                        <span className={`text-lg font-medium transition-transform duration-300 ${isActive ? 'translate-x-1' : ''}`}>
                          {item.title || kindLabel(item.kind)}
                        </span>
                        <span className="text-[10px] uppercase tracking-widest text-zinc-500">
                          {kindLabel(item.kind)} · {kindDetail(item.kind)}
                        </span>
                      </div>
                      {isActive && (
                        <div className="ml-auto flex items-center gap-2">
                          <span className="text-[10px] font-bold text-accent-primary/50 uppercase tracking-tighter">On Air</span>
                          <div className="w-2 h-2 rounded-full bg-accent-primary animate-ping" />
                        </div>
                      )}
                    </button>
                  );
                })
              ) : (
                <div className="p-6 text-sm text-zinc-500">No session items yet.</div>
              )}
            </div>
          </div>
        </section>

        <aside className="col-span-4 flex flex-col gap-6 animate-in" style={{ animationDelay: '0.1s' }}>
          <section className="bg-bg-card rounded-2xl border border-white/5 p-6 shadow-2xl space-y-4">
            <div className="flex items-center justify-between">
              <h3 className="text-[10px] font-black uppercase tracking-widest text-zinc-500">Project</h3>
              <span className="text-[10px] text-zinc-600 font-mono">{ipcLabel}</span>
            </div>

            <div className="space-y-2">
              <div className="flex gap-2">
                <input
                  value={projectName}
                  onChange={(e) => setProjectName(e.target.value)}
                  placeholder="New project name"
                  className="flex-1 h-10 rounded-xl bg-bg-hover border border-white/5 px-3 text-sm text-white placeholder:text-zinc-600 outline-none"
                />
                <button
                  onClick={async () => {
                    const name = projectName.trim();
                    if (!name) return;
                    const created = await invoke<ProjectModel>('create_project', { name });
                    setProjectName('');
                    await invoke('save_project', { project: created });
                    const list = await invoke<ProjectHeader[]>('list_projects');
                    if (Array.isArray(list)) setProjects(list);
                    await invoke('load_project', { project_id: created.id });
                  }}
                  className="h-10 px-4 rounded-xl bg-white text-black hover:bg-zinc-200 text-xs font-black uppercase tracking-widest active:scale-95 transition"
                >
                  Create
                </button>
              </div>

              {projects.length === 0 && (
                <button
                  onClick={createDemoProject}
                  className="w-full h-10 rounded-xl bg-bg-hover hover:bg-zinc-700 border border-white/5 text-xs font-black uppercase tracking-widest text-zinc-300 active:scale-95 transition"
                >
                  Create Demo Session
                </button>
              )}

              <div className="space-y-1">
                {projects.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => invoke('load_project', { project_id: p.id })}
                    className={`w-full text-left px-3 py-2 rounded-xl border transition ${
                      project?.id === p.id
                        ? 'bg-accent-primary/10 border-accent-primary/20 text-white'
                        : 'bg-bg-hover border-white/5 text-zinc-400 hover:text-white'
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <span className="text-sm font-semibold">{p.name}</span>
                      <span className="text-[10px] font-mono text-zinc-600">#{p.id}</span>
                    </div>
                  </button>
                ))}
              </div>
            </div>
          </section>

          <section className="bg-bg-card rounded-2xl border border-white/5 p-6 shadow-2xl space-y-6">
            <div className="flex gap-3">
              <button
                onClick={handlePrev}
                className="flex-1 bg-bg-hover hover:bg-zinc-700 h-12 rounded-xl flex items-center justify-center transition-all border border-white/5 active:scale-95"
                title="Prev"
              >
                <svg className="w-5 h-5 text-zinc-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                </svg>
              </button>
              <button
                onClick={togglePlay}
                className="flex-[2] bg-white text-black hover:bg-zinc-200 h-12 rounded-xl font-bold flex items-center justify-center gap-2 transition-all active:scale-95"
              >
                {status === 'Playing' ? (
                  <>
                    <div className="w-2 h-4 bg-black rounded-sm" />
                    <div className="w-2 h-4 bg-black rounded-sm" />
                  </>
                ) : (
                  <svg className="w-5 h-5 fill-current" viewBox="0 0 24 24">
                    <path d="M8 5v14l11-7z" />
                  </svg>
                )}
                <span>{transport === 'playing' ? 'PAUSE' : 'PLAY'}</span>
              </button>
              <button
                onClick={handleNext}
                className="flex-1 bg-bg-hover hover:bg-zinc-700 h-12 rounded-xl flex items-center justify-center transition-all border border-white/5 active:scale-95"
                title="Next"
              >
                <svg className="w-5 h-5 text-zinc-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                </svg>
              </button>
            </div>

            <div className="space-y-2">
              <div className="flex items-center justify-between">
                <h3 className="text-[10px] font-black uppercase tracking-widest text-zinc-500">Active</h3>
                <div className="text-[10px] text-zinc-600 font-mono">{clock}</div>
              </div>
              <div className="rounded-xl bg-bg-hover border border-white/5 p-4">
                <div className="text-sm font-semibold text-white/90">
                  {activeItem ? activeItem.title || kindLabel(activeItem.kind) : '—'}
                </div>
                <div className="mt-2 text-xs text-zinc-500">{activeItem ? `Type: ${kindLabel(activeItem.kind)}` : 'Load a project to begin.'}</div>
                <div className="mt-3 text-sm text-white/80 leading-relaxed">{activeText || '—'}</div>
              </div>
            </div>
          </section>

          <section className="bg-bg-card rounded-2xl border border-white/5 p-6 shadow-2xl space-y-3">
            <div className="flex items-center justify-between">
              <h3 className="text-[10px] font-black uppercase tracking-widest text-zinc-500">Item Controls</h3>
              <div className="text-[10px] text-zinc-600 font-mono">
                {programState?.active_step_index != null ? `step ${programState.active_step_index + 1}` : ''}
              </div>
            </div>

            {activeItem?.kind.kind === 'text' ? (
              <div className="space-y-1 max-h-64 overflow-y-auto custom-scrollbar">
                {activeItem.kind.data.steps.map((s, i) => {
                  const isActive = programState?.active_step_index === i;
                  return (
                    <button
                      key={i}
                      onClick={() => invoke('go_to_step', { index: i })}
                      className={`w-full text-left px-4 py-3 rounded-xl border transition ${
                        isActive
                          ? 'bg-accent-primary/10 border-accent-primary/20 text-white'
                          : 'bg-bg-hover border-white/5 text-zinc-400 hover:text-white'
                      }`}
                    >
                      <div className="text-[10px] font-mono text-zinc-600">{String(i + 1).padStart(2, '0')}</div>
                      <div className="text-sm font-medium">{s}</div>
                    </button>
                  );
                })}
              </div>
            ) : (
              <div className="text-sm text-zinc-400">
                Text items show step buttons here. Image/video items apply when activated in the playlist.
              </div>
            )}
          </section>
        </aside>
      </main>

      <footer className="h-10 glass border-t border-white/5 flex items-center justify-between px-8 text-[10px] text-zinc-600 font-mono uppercase tracking-[0.2em]">
        <span>Engine: WGPU-DIRECT-V1</span>
        <span>Local Time: {clock}</span>
        <div className="flex gap-4">
          <span className="text-emerald-500/50">Network Active</span>
          <span className="text-accent-primary/50">{ipcLabel}</span>
        </div>
      </footer>
    </div>
  );
}

export default App;
