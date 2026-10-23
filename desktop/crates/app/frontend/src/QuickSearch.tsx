import { useMemo, useState } from 'react';
import { Icon } from './Icons';
import type { Media, SongHeader } from './models';
import { fileName, searchLibrary, type SearchResult } from './search';

/** Ctrl+K: find a song or media file and add it to the service. */
export default function QuickSearch({ songs, media, canAdd, onAdd, onClose }: { songs: SongHeader[]; media: Media[]; canAdd: boolean; onAdd: (result: SearchResult) => Promise<void>; onClose: () => void }) {
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const results = useMemo(() => searchLibrary(query, songs, media), [query, songs, media]);
  const add = async (result: SearchResult | undefined) => {
    if (!result || busy) return;
    setBusy(true); setError('');
    try { await onAdd(result); onClose(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <div className="modal-backdrop" onMouseDown={e => { if (e.target === e.currentTarget) onClose(); }}><section className="quick-search" role="dialog" aria-modal="true" aria-label="Quick search" onKeyDown={e => {
    if (e.key === 'Escape') { e.preventDefault(); onClose(); }
    if (e.key === 'ArrowDown') { e.preventDefault(); setActive(i => Math.min(results.length - 1, i + 1)); }
    if (e.key === 'ArrowUp') { e.preventDefault(); setActive(i => Math.max(0, i - 1)); }
    if (e.key === 'Enter') { e.preventDefault(); void add(results[active]); }
  }}>
    <input autoFocus aria-label="Search songs and media" placeholder={canAdd ? 'Find a song or media file to add…' : 'Select a service first'} value={query} onChange={e => { setQuery(e.target.value); setActive(0); }} disabled={!canAdd}/>
    {error && <p className="ui-error" role="alert">{error}</p>}
    <ul role="listbox" aria-label="Results">{results.map((result, index) => {
      const key = result.kind === 'song' ? `song-${result.song.id}` : `media-${result.media.id}`;
      const title = result.kind === 'song' ? result.song.title : result.media.title;
      const detail = result.kind === 'song' ? 'Song' : `${result.media.kind} · ${fileName(result.media.path)}`;
      return <li key={key} role="option" aria-selected={index === active} className={index === active ? 'is-active' : ''} onMouseEnter={() => setActive(index)} onClick={() => void add(result)}>
        <Icon name={result.kind === 'song' ? 'library' : result.media.kind === 'image' ? 'logo' : result.media.kind}/><span>{title}<small>{detail}</small></span>
      </li>;
    })}</ul>
    {!results.length && <p className="empty-panel">{query ? 'Nothing matches.' : 'The library is empty.'}</p>}
    <footer><kbd>↑</kbd><kbd>↓</kbd> choose · <kbd>Enter</kbd> add to the service · <kbd>Esc</kbd> close</footer>
  </section></div>;
}
