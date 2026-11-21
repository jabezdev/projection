import { Icon } from '../ui/Icons';
import type { MediaKind, SessionItem } from '../api/models';
import { itemSong, songSteps } from '../song/song';
import { Background, SlidePreview } from '../song/SlidePreview';

export function MediaThumb({ kind, path, badge }: { kind: MediaKind; path: string; badge?: string }) {
  return <div className="thumb">
    {kind === 'audio' ? <span className="thumb-tile"><Icon name="audio"/></span> : <Background path={path} fit="cover"/>}
    {kind === 'video' && !badge && <span className="thumb-badge"><Icon name="video"/></span>}
    {badge && <span className="thumb-badge">{badge}</span>}
  </div>;
}

export function ItemThumb({ item }: { item: SessionItem }) {
  const kind = item.kind;
  if (kind.kind === 'text') {
    const song = itemSong(item);
    return <div className="thumb"><SlidePreview text={songSteps(song)[0] || ''} background={song.background || item.background || ''}/></div>;
  }
  if (kind.kind === 'presentation') {
    const slides = kind.data.slides || [kind.data.path];
    return <div className="thumb"><Background path={slides[0] || ''} fit="contain"/><span className="thumb-badge">{slides.length}</span></div>;
  }
  if (kind.kind === 'section') return <div className="thumb"><span className="thumb-tile"><small>Section</small></span></div>;
  if (kind.kind === 'capture') return <div className="thumb"><span className="thumb-tile"><Icon name="capture"/><small>{kind.data.source.kind === 'window' ? 'Window' : 'Screen'}</small></span></div>;
  return <MediaThumb kind={kind.kind} path={kind.data.path}/>;
}
