import { useState } from 'react';
import { Icon } from './Icons';

export type PartEdit = { itemId: number; partId: string; label: string; lyrics: string };

/** Edits one part's lyrics in the service copy of a song. */
export default function PartEditor({ part, onClose, onSave }: { part: PartEdit; onClose: () => void; onSave: (part: PartEdit) => Promise<void> }) {
  const [lyrics, setLyrics] = useState(part.lyrics);
  const [saving, setSaving] = useState(false);
  return <div className="modal-backdrop" onKeyDown={e => {
    if (e.key === 'Escape' && !saving) onClose();
    if (e.key === 'Tab') {
      const targets = e.currentTarget.querySelectorAll<HTMLElement>('button:not(:disabled), textarea');
      const first = targets[0], last = targets[targets.length - 1];
      if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
      if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
    }
  }}><form className="compact-modal part-edit-modal" role="dialog" aria-modal="true" aria-label={`Edit ${part.label}`} onSubmit={e => { e.preventDefault(); setSaving(true); void onSave({ ...part, lyrics }).finally(() => setSaving(false)); }}>
    <header><h2>{part.label}</h2><button type="button" aria-label="Close part editor" disabled={saving} onClick={onClose}><Icon name="close"/></button></header>
    <textarea autoFocus aria-label="Part lyrics" value={lyrics} onChange={e => setLyrics(e.target.value)}/>
    <small>A blank line creates a new slide. Changes apply to every occurrence of this part.</small>
    <div className="modal-actions"><button type="button" disabled={saving} onClick={onClose}>Cancel</button><button className="primary-action" disabled={saving}>{saving ? 'Saving…' : 'Save part'}</button></div>
  </form></div>;
}
