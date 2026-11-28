import { useState } from 'react';
import type { ItemContent, ServiceItem } from '../api/models';
import { useBibles, usePassage, writePreference } from '../api/bible';
import { PassagePreview, TranslationPicker } from './ScriptureFields';
import { Icon } from '../ui/Icons';

type Scripture = Extract<ItemContent, { kind: 'scripture' }>;

export default function ScriptureEditor({ item, content, onContent, onTitle }: {
  item: ServiceItem; content: Scripture; onContent: (next: Scripture, delay?: number) => void; onTitle: (title: string) => void;
}) {
  const bibles = useBibles();
  const [reference, setReference] = useState(content.reference);
  const passage = usePassage(reference, content.translation, content.parallel ?? '');
  const choose = (translation: string, parallel: string) => {
    writePreference({ translation, parallel });
    onContent({ ...content, translation, parallel: parallel || undefined }, 0);
  };
  return <div className="item-layout">
    <div className="item-layout-top">
      <div className="item-preview"><div className="thumb"><span className="thumb-tile"><Icon name="bible"/><small>Bible</small></span></div></div>
      <div className="item-layout-head"><span className="eyebrow">Bible passage</span>
        <label className="item-field item-title">Title<input aria-label="Item title" placeholder={passage?.title ?? 'Shown title'} value={item.title || ''} onChange={e => onTitle(e.target.value)}/></label>
      </div>
    </div>
    <label className="item-field">Reference <small>Like John 3:16-18, Ps 23 or Juan 3:16; Rom 5:8</small>
      <input aria-label="Bible reference" value={reference} placeholder="John 3:16" onChange={e => { setReference(e.target.value); onContent({ ...content, reference: e.target.value }); }}/>
    </label>
    <TranslationPicker bibles={bibles} translation={content.translation} parallel={content.parallel ?? ''} onTranslation={id => choose(id, content.parallel === id ? '' : content.parallel ?? '')} onParallel={id => choose(content.translation, id)}/>
    <p className="field-hint">One verse per slide. Long verses get smaller, then continue on another slide. Imported Bibles stay on this computer; another computer without one shows the first bundled Bible instead.</p>
    <PassagePreview passage={passage} empty="Type a reference to see the verses."/>
  </div>;
}
