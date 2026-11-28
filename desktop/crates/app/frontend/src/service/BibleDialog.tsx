import { useState } from 'react';
import type { Service } from '../api/models';
import { invoke } from '../api/tauri';
import { readPreference, useBibles, useBibleSearch, usePassage, writePreference } from '../api/bible';
import { PassagePreview, TranslationPicker } from './ScriptureFields';
import { scriptureItem } from './serviceItems';
import { Icon } from '../ui/Icons';

export default function BibleDialog({ service, activeItemId, persist, onClose }: {
  service: Service | null; activeItemId?: string | null; persist: (service: Service) => Promise<void>; onClose: () => void;
}) {
  const bibles = useBibles();
  const [preference, setPreference] = useState(readPreference);
  const [query, setQuery] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const isReference = /\d/.test(query);
  const passage = usePassage(isReference ? query : '', preference.translation, preference.parallel);
  const hits = useBibleSearch(isReference ? '' : query, preference.translation);
  const ready = !!passage?.slides.length && !!service;
  const choose = (next: typeof preference) => { setPreference(next); writePreference(next); };

  async function add(show: boolean) {
    if (!service || !passage?.slides.length) return;
    setBusy(true); setError('');
    try {
      const item = scriptureItem(query.trim(), preference.translation, preference.parallel || undefined);
      const after = show && activeItemId ? service.items.findIndex(i => i.id === activeItemId) + 1 : service.items.length;
      const at = after > 0 ? after : service.items.length;
      await persist({ ...service, items: [...service.items.slice(0, at), item, ...service.items.slice(at)] });
      if (show) await invoke('cue', { itemId: item.id, index: 0 });
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  }

  return <div className="modal-backdrop" onKeyDown={e => { if (e.key === 'Escape') { e.stopPropagation(); onClose(); } }}>
    <form className="compact-modal bible-dialog" role="dialog" aria-modal="true" aria-label="Bible" onSubmit={e => { e.preventDefault(); void add(true); }}>
      <header className="bible-dialog-head"><Icon name="bible"/><h2>Bible</h2><button type="button" className="icon-button" aria-label="Close" onClick={onClose}><Icon name="close"/></button></header>
      <input autoFocus aria-label="Reference or words" placeholder="John 3:16, Awit 23, or words like “shepherd”" value={query} onChange={e => setQuery(e.target.value)}/>
      <TranslationPicker bibles={bibles} translation={preference.translation} parallel={preference.parallel}
        onTranslation={id => choose({ translation: id, parallel: preference.parallel === id ? '' : preference.parallel })} onParallel={id => choose({ ...preference, parallel: id })}/>
      <div className="bible-dialog-body">
        {isReference || !query.trim()
          ? <PassagePreview passage={passage} empty="Type a reference to see it, or a few words to search."/>
          : <ul className="bible-hits">{hits === null ? <li className="field-hint">Keep typing to search…</li> : !hits.length ? <li className="field-hint">No verses have all these words.</li>
            : hits.map(hit => <li key={hit.reference}><button type="button" onClick={() => setQuery(hit.reference)}><strong>{hit.reference}</strong><span>{hit.text}</span></button></li>)}</ul>}
      </div>
      {!service && <p className="field-hint">Load a service first.</p>}
      {error && <p className="passage-warning" role="alert">{error}</p>}
      <div className="modal-actions">
        <button type="button" disabled={!ready || busy} onClick={() => void add(false)}>Add to the end</button>
        <button type="submit" className="primary-action" disabled={!ready || busy} title="Adds it after the item on air and shows the first verse">Show now</button>
      </div>
    </form>
  </div>;
}
