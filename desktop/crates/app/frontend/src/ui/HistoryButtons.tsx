import { Icon } from './Icons';
import type { History } from '../api/history';

type Props = { history: History; busy?: boolean; blocked?: string; onStep: (redo: boolean) => void };

export default function HistoryButtons({ history, busy, blocked, onStep }: Props) {
  const title = (verb: string, label: string | undefined, keys: string) => blocked ?? (label ? `${verb}: ${label} (${keys})` : `Nothing to ${verb.toLowerCase()}`);
  return <span className="history-buttons">
    <button className="icon-button" aria-label={history.undo ? `Undo ${history.undo}` : 'Undo'} title={title('Undo', history.undo, 'Ctrl+Z')} disabled={busy || !!blocked || !history.undo} onClick={() => onStep(false)}><Icon name="undo"/></button>
    <button className="icon-button" aria-label={history.redo ? `Redo ${history.redo}` : 'Redo'} title={title('Redo', history.redo, 'Ctrl+Shift+Z')} disabled={busy || !!blocked || !history.redo} onClick={() => onStep(true)}><Icon name="redo"/></button>
  </span>;
}
