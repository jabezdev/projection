// PALTO phone remote: pairs once with the code on the operator screen, then
// drives the engine over the WebSocket with the same commands the operator
// window uses. The token is kept in this browser.
'use strict';

const TOKEN = 'palto.token';
const $ = id => document.getElementById(id);
let socket = null;
let project = null;
let program = null;
let retry = 0;

const storage = {
  get() { try { return localStorage.getItem(TOKEN); } catch { return null; } },
  set(value) { try { value ? localStorage.setItem(TOKEN, value) : localStorage.removeItem(TOKEN); } catch { /* private mode: pair again next time */ } },
};

function show(screen) {
  $('pair').hidden = screen !== 'pair';
  $('remote').hidden = screen !== 'remote';
}

async function check() {
  const token = storage.get();
  if (!token) return show('pair');
  try {
    const response = await fetch(`/api/check?token=${encodeURIComponent(token)}`);
    if (response.status === 401) { storage.set(null); return show('pair'); }
  } catch { /* offline: try the socket anyway */ }
  show('remote');
  connect();
}

$('pair-form').addEventListener('submit', async event => {
  event.preventDefault();
  $('pair-error').textContent = '';
  try {
    const response = await fetch('/api/pair', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ code: $('code').value, name: $('name').value }) });
    const body = await response.json();
    if (!response.ok) { $('pair-error').textContent = body.error || 'Pairing failed'; return; }
    storage.set(body.token);
    show('remote');
    connect();
  } catch { $('pair-error').textContent = 'Cannot reach PALTO. Is this device on the same network?'; }
});

function connect() {
  const token = storage.get() || '';
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
  socket = new WebSocket(`${scheme}://${location.host}/ws?token=${encodeURIComponent(token)}`);
  setStatus('Connecting…', false);
  socket.addEventListener('open', () => { retry = 0; setStatus('Connected', true); });
  socket.addEventListener('message', message => receive(JSON.parse(message.data)));
  socket.addEventListener('close', () => {
    setStatus('Reconnecting…', false);
    socket = null;
    // The token may have been removed on the operator screen.
    setTimeout(check, Math.min(10000, 500 * 2 ** retry++));
  });
}

function setStatus(text, ok) {
  $('connection').textContent = text;
  $('connection').classList.toggle('ok', ok);
  document.querySelectorAll('button[data-display], #prev, #next-button').forEach(b => { b.disabled = !ok; });
}

function send(type, payload) {
  if (socket?.readyState !== WebSocket.OPEN) return;
  socket.send(JSON.stringify(payload === undefined ? { type } : { type, payload }));
  if (navigator.vibrate) navigator.vibrate(15);
}

function receive(event) {
  if (event.type === 'project_changed') project = event.payload.project;
  else if (event.type === 'program_changed') program = event.payload.program_state;
  else if (event.type === 'error' && event.payload.source === 'remote') { $('error').textContent = event.payload.message; setTimeout(() => { $('error').textContent = ''; }, 3000); return; }
  else return;
  render();
}

const items = () => project?.session?.items ?? [];
const label = item => item?.title || ({ text: 'Song', image: 'Image', video: 'Video', audio: 'Audio', presentation: 'Presentation' })[item?.kind?.kind] || '';
function steps(item) {
  const kind = item?.kind;
  if (kind?.kind === 'text') return kind.data.steps;
  if (kind?.kind === 'presentation') return (kind.data.slides ?? []).map((_, i) => `Slide ${i + 1}`);
  return [];
}

function render() {
  $('service').textContent = project?.name ?? 'No service loaded';
  const list = items();
  const index = list.findIndex(item => item.id === program?.active_item_id);
  const item = list[index];
  const step = program?.active_step_index ?? 0;
  const parts = steps(item);
  $('current-label').textContent = item ? `Now · ${label(item)}${parts.length > 1 ? ` · ${step + 1}/${parts.length}` : ''}` : 'Now';
  $('current').textContent = item ? (program.active_text || parts[step] || label(item)) : 'Nothing cued';
  let next = '';
  let nextLabel = 'Next';
  if (item && step + 1 < parts.length) next = parts[step + 1];
  else {
    const following = list[index + 1];
    if (following) { nextLabel = `Next · ${label(following)}`; next = steps(following)[0] || label(following); }
  }
  $('next-label').textContent = nextLabel;
  $('next').textContent = next || (item ? 'End of service' : '');
  const display = program?.display ?? 'live';
  document.querySelectorAll('button[data-display]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.display === display)));
}

document.querySelectorAll('button[data-display]').forEach(button => button.addEventListener('click', () => send('set_display', { display: button.dataset.display })));
$('prev').addEventListener('click', () => send('prev'));
$('next-button').addEventListener('click', () => send('next'));
// Volume keys are not available to web pages; arrow keys help on tablets with keyboards.
document.addEventListener('keydown', event => {
  if ($('remote').hidden || event.target instanceof HTMLInputElement) return;
  if (event.key === 'ArrowRight' || event.key === ' ' || event.key === 'PageDown') { event.preventDefault(); send('next'); }
  if (event.key === 'ArrowLeft' || event.key === 'PageUp') { event.preventDefault(); send('prev'); }
});

setStatus('Connecting…', false);
check();
