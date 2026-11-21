import { useEffect, useState } from 'react';
import { invoke, isTauri, listen } from './tauri';

/** Mirrors `protocol::InputAction`; Rust turns each into engine commands. */
export type InputAction = 'next' | 'previous' | 'next_item' | 'previous_item' | 'live' | 'black' | 'clear' | 'logo' | 'play_pause' | 'stop';
export type MidiMessage = 'note' | 'control' | 'program';
export type InputTrigger =
  | { device: 'key'; key: string; ctrl: boolean; alt: boolean; shift: boolean }
  | { device: 'midi'; message: MidiMessage; channel: number; number: number }
  | { device: 'stream_deck'; button: number };
export type InputBinding = { trigger: InputTrigger; action: InputAction };
export type InputBindings = { version: number; bindings: InputBinding[] };

export const actionLabels: Record<InputAction, string> = {
  next: 'Next', previous: 'Previous', next_item: 'Next item', previous_item: 'Previous item',
  live: 'Live', black: 'Black (toggle)', clear: 'Clear (toggle)', logo: 'Logo (toggle)', play_pause: 'Play / pause media', stop: 'Stop media',
};
export const inputActions = Object.keys(actionLabels) as InputAction[];

/** The key trigger for a keyboard event: single characters lowercase, the space bar as `Space`. */
export function keyTrigger(event: KeyboardEvent): InputTrigger {
  const key = event.key === ' ' ? 'Space' : event.key.length === 1 ? event.key.toLowerCase() : event.key;
  return { device: 'key', key, ctrl: event.ctrlKey || event.metaKey, alt: event.altKey, shift: event.shiftKey };
}

const keyNames: Record<string, string> = { ArrowRight: '→', ArrowLeft: '←', ArrowUp: '↑', ArrowDown: '↓', Space: 'Space', PageDown: 'Page Down', PageUp: 'Page Up', Escape: 'Esc' };
export function triggerLabel(trigger: InputTrigger): string {
  if (trigger.device === 'key') {
    const key = keyNames[trigger.key] || (trigger.key.length === 1 ? trigger.key.toUpperCase() : trigger.key);
    return [trigger.ctrl && 'Ctrl', trigger.alt && 'Alt', trigger.shift && 'Shift', key].filter(Boolean).join('+');
  }
  if (trigger.device === 'midi') return `MIDI ${trigger.message === 'note' ? 'note' : trigger.message === 'control' ? 'CC' : 'program'} ${trigger.number} · ch ${trigger.channel}`;
  return `Stream Deck key ${trigger.button + 1}`;
}

export const sameTrigger = (a: InputTrigger, b: InputTrigger) => JSON.stringify(a) === JSON.stringify(b);

/** Keys that only mean something in a text field, or while a dialog is open, never trigger actions. */
function ignored(event: KeyboardEvent) {
  const target = event.target as HTMLElement | null;
  if (event.defaultPrevented || event.isComposing || event.repeat) return true;
  if (target?.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]')) return true;
  return !!document.querySelector('[aria-modal="true"]');
}

/** The saved bindings, kept current. */
export function useInputBindings(): InputBindings | null {
  const [bindings, setBindings] = useState<InputBindings | null>(null);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void listen<InputBindings>('input:bindings', ({ payload }) => { if (!disposed) setBindings(payload); }).then(unlisten => { if (disposed) unlisten(); else cleanup = unlisten; });
    void invoke<InputBindings>('get_input_bindings').then(value => { if (!disposed) setBindings(value); }).catch(console.error);
    return () => { disposed = true; cleanup?.(); };
  }, []);
  return bindings;
}

/** Sends the action bound to each key press made outside text fields and dialogs. */
export function useKeyboardInput(bindings: InputBindings | null, onError: (message: string) => void) {
  useEffect(() => {
    if (!bindings) return;
    const onKey = (event: KeyboardEvent) => {
      if (ignored(event)) return;
      const trigger = keyTrigger(event);
      const binding = bindings.bindings.find(b => sameTrigger(b.trigger, trigger));
      if (!binding) return;
      // Also keeps Space from pressing a focused button.
      event.preventDefault();
      void invoke('trigger_action', { action: binding.action }).catch(e => onError(String(e)));
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [bindings, onError]);
}
