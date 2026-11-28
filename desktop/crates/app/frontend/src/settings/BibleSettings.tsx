import { useState } from 'react';
import { invoke, isTauri } from '../api/tauri';
import type { BibleInfo } from '../api/models';
import { readPreference, useBibles, writePreference } from '../api/bible';
import { TranslationPicker } from '../service/ScriptureFields';
import { Icon } from '../ui/Icons';

export default function BibleSettings() {
  const [revision, setRevision] = useState(0);
  const bibles = useBibles(revision);
  const [preference, setPreference] = useState(readPreference);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState('');
  async function importBible() {
    setBusy(true); setMessage('');
    try {
      const info = await invoke<BibleInfo | null>('import_bible');
      if (info) { setMessage(`Imported ${info.name} (${info.abbreviation}).`); setRevision(r => r + 1); }
    } catch (e) { setMessage(String(e)); } finally { setBusy(false); }
  }
  async function remove(bible: BibleInfo) {
    try { await invoke('remove_bible', { id: bible.id }); setRevision(r => r + 1); } catch (e) { setMessage(String(e)); }
  }
  const choose = (next: typeof preference) => { setPreference(next); writePreference(next); };
  return <div className="settings-fields bible-settings">
    <p className="field-hint">New Bible passages use these. Each passage can still pick its own.</p>
    <TranslationPicker bibles={bibles} translation={preference.translation} parallel={preference.parallel}
      onTranslation={id => choose({ translation: id, parallel: preference.parallel === id ? '' : preference.parallel })} onParallel={id => choose({ ...preference, parallel: id })}/>
    <h3>Bibles on this computer</h3>
    <ul className="bible-list">{bibles.map(b => <li key={b.id}>
      <strong>{b.abbreviation}</strong><span>{b.name}</span><small>{b.bundled ? 'Included' : 'Imported'} · {b.language}</small>
      {!b.bundled && <button className="icon-button" aria-label={`Remove ${b.name}`} onClick={() => void remove(b)}><Icon name="trash"/></button>}
    </li>)}</ul>
    <div><button disabled={busy || !isTauri()} title={isTauri() ? undefined : 'Needs the desktop app'} onClick={() => void importBible()}><Icon name="import"/>{busy ? 'Importing…' : 'Import a Bible…'}</button></div>
    <p className="field-hint">Zefania XML, OSIS XML or USFM (a file or a zip of book files). Only import translations the church has a license for. Imported Bibles stay on this computer and never sync.</p>
    {message && <p role="status" className="field-hint">{message}</p>}
  </div>;
}
