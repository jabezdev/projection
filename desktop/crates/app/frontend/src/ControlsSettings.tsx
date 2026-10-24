import { useEffect, useState } from 'react';
import { invoke, listen } from './tauri';
import { actionLabels, inputActions, keyTrigger, sameTrigger, triggerLabel, type InputAction, type InputBindings, type InputTrigger } from './input';
import { Icon } from './Icons';

type Devices = { midi: string[]; stream_decks: string[] };

/** The bindings editor: keys, MIDI and Stream Deck buttons for each action. */
export default function ControlsSettings({ bindings }: { bindings: InputBindings | null }) {
  const [learning, setLearning] = useState<InputAction | null>(null);
  const [devices, setDevices] = useState<Devices>({ midi: [], stream_decks: [] });
  const [message, setMessage] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    const load = () => void invoke<Devices>('get_input_devices').then(setDevices).catch(() => {});
    load();
    const timer = window.setInterval(load, 3000);
    return () => window.clearInterval(timer);
  }, []);

  const save = async (next: InputBindings) => {
    setError('');
    try { await invoke('set_input_bindings', { bindings: next }); } catch (e) { setError(String(e)); }
  };
  const bind = async (action: InputAction, trigger: InputTrigger) => {
    if (!bindings) return;
    const previous = bindings.bindings.find(b => sameTrigger(b.trigger, trigger));
    setMessage(previous && previous.action !== action ? `${triggerLabel(trigger)} moved from ${actionLabels[previous.action]} to ${actionLabels[action]}.` : `${triggerLabel(trigger)} now does ${actionLabels[action]}.`);
    await save({ ...bindings, bindings: [...bindings.bindings.filter(b => !sameTrigger(b.trigger, trigger)), { trigger, action }] });
  };

  // While learning: the next key here, or the next MIDI / Stream Deck press from Rust.
  useEffect(() => {
    if (!learning) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    const finish = (trigger: InputTrigger | null) => {
      if (disposed) return;
      disposed = true;
      setLearning(null);
      void invoke('learn_input', { learning: false });
      if (trigger) void bind(learning, trigger);
    };
    const onKey = (event: KeyboardEvent) => {
      // Captured before the operator's own shortcuts and the dialog's Escape.
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape' && !event.shiftKey) { finish(null); return; }
      if (['Control', 'Shift', 'Alt', 'Meta'].includes(event.key)) return;
      finish(keyTrigger(event));
    };
    window.addEventListener('keydown', onKey, true);
    void invoke('learn_input', { learning: true });
    void listen<InputTrigger>('input:learned', ({ payload }) => finish(payload)).then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; });
    return () => { disposed = true; window.removeEventListener('keydown', onKey, true); cleanup?.(); void invoke('learn_input', { learning: false }); };
    // `bind` changes with the bindings; learning restarts only per action.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [learning]);

  if (!bindings) return <div className="settings-fields"><p>Loading controls…</p></div>;
  return <div className="settings-fields controls-settings">
    {error && <p role="alert" className="ui-error">{error}</p>}
    <fieldset className="controls-bindings"><legend>Actions</legend>
      <p className="controls-help">Keys work while the operator window is focused and no dialog is open (typing in a field never triggers them). Presenter clickers usually send Page Up / Page Down.</p>
      {inputActions.map(action => <div className="control-row" key={action}>
        <span className="control-action">{actionLabels[action]}</span>
        <span className="control-triggers">{bindings.bindings.filter(b => b.action === action).map(b => <span className="control-chip" key={JSON.stringify(b.trigger)}>{triggerLabel(b.trigger)}<button aria-label={`Remove ${triggerLabel(b.trigger)} from ${actionLabels[action]}`} onClick={() => void save({ ...bindings, bindings: bindings.bindings.filter(x => !sameTrigger(x.trigger, b.trigger)) })}><Icon name="close"/></button></span>)}</span>
        <button className={learning === action ? 'is-learning' : ''} onClick={() => setLearning(learning === action ? null : action)}>{learning === action ? 'Press a key or button… (Esc cancels)' : 'Add'}</button>
      </div>)}
      {message && <small role="status">{message}</small>}
      <button onClick={() => void invoke('reset_input_bindings').then(() => setMessage('The default keys are back.')).catch(e => setError(String(e)))}>Reset to the default keys</button>
    </fieldset>
    <fieldset><legend>Devices</legend>
      <small>MIDI inputs: {devices.midi.length ? devices.midi.join(', ') : 'none connected'}</small>
      <small>Stream Decks: {devices.stream_decks.length ? devices.stream_decks.join(', ') : 'none connected'}</small>
      <small>Devices connected later are found within a few seconds. To bind one, press Add, then the controller's key or pad.</small>
    </fieldset>
  </div>;
}
