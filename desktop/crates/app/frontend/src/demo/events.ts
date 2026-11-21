

// Desktop-only events (output:shown and the like), raised by the fake commands.
export const uiOnlyEvents = new EventTarget();
export function emitUiOnly(event: string, payload: unknown): void {
  uiOnlyEvents.dispatchEvent(new CustomEvent(event, { detail: payload }));
}
