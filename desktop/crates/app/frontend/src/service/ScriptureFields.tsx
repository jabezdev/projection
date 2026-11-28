import type { BibleInfo, Passage } from '../api/models';

export function TranslationPicker({ bibles, translation, parallel, onTranslation, onParallel }: {
  bibles: BibleInfo[]; translation: string; parallel: string; onTranslation: (id: string) => void; onParallel: (id: string) => void;
}) {
  const known = (id: string) => !id || bibles.some(b => b.id === id);
  return <div className="scripture-translations">
    <label>Translation<select aria-label="Translation" value={translation} onChange={e => onTranslation(e.target.value)}>
      {!known(translation) && <option value={translation}>{translation.toUpperCase()} (not on this computer)</option>}
      {bibles.map(b => <option key={b.id} value={b.id}>{b.abbreviation} · {b.name}</option>)}
    </select></label>
    <label>Side by side<select aria-label="Second translation" value={parallel} onChange={e => onParallel(e.target.value)}>
      <option value="">None</option>
      {!known(parallel) && <option value={parallel}>{parallel.toUpperCase()} (not on this computer)</option>}
      {bibles.filter(b => b.id !== translation).map(b => <option key={b.id} value={b.id}>{b.abbreviation} · {b.name}</option>)}
    </select></label>
  </div>;
}

export function PassagePreview({ passage, empty }: { passage: Passage | null; empty: string }) {
  if (!passage) return <p className="field-hint">{empty}</p>;
  return <div className="passage-preview">
    {passage.warning && <p className="passage-warning" role="status">{passage.warning}</p>}
    {passage.slides.length > 0 && <p className="passage-title">{passage.title} · {passage.slides.length} slide{passage.slides.length === 1 ? '' : 's'}</p>}
    <ol className="passage-verses">{passage.slides.map((slide, index) => <li key={index}>
      <small>{slide.caption.split(' · ')[0]}</small>
      <span>{slide.text}</span>
      {slide.parallel && <span className="passage-parallel">{slide.parallel}</span>}
    </li>)}</ol>
  </div>;
}
