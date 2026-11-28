'use strict';

const TOKEN = 'palto.token';
const TAB = 'palto.tab';
const $ = id => document.getElementById(id);
const state = { live: null, program: null, playback: null, playbackAt: 0, open: new Set(), tab: 'live' };
let socket = null;
let retry = 0;

const storage = {
  get(key) { try { return localStorage.getItem(key); } catch { return null; } },
  set(key, value) { try { value ? localStorage.setItem(key, value) : localStorage.removeItem(key); } catch {  } },
};

const SECTIONS = {
  verse: ['V', '#4f91e8', 'Verse'], pre_chorus: ['PC', '#efa34a', 'Pre-chorus'], chorus: ['C', '#e76774', 'Chorus'], bridge: ['BR', '#aa84df', 'Bridge'],
  intro: ['I', '#50c6d6', 'Intro'], outro: ['O', '#ddbd53', 'Outro'], tag: ['T', '#79c56b', 'Tag'], other: ['', '#a3a8b2', 'Part'],
};
const KINDS = { text: 'Text', image: 'Image', video: 'Video', audio: 'Audio', presentation: 'Slides', capture: 'Screen' };

function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key === 'style') node.style.cssText = value;
    else if (key.startsWith('on')) node.addEventListener(key.slice(2), value);
    else if (value !== undefined && value !== null && value !== false) node.setAttribute(key, value === true ? '' : value);
  }
  node.append(...children.flat().filter(c => c !== null && c !== undefined && c !== false));
  return node;
}

function show(screen) {
  $('pair').hidden = screen !== 'pair';
  $('remote').hidden = screen !== 'remote';
}

async function check() {
  const token = storage.get(TOKEN);
  if (!token) return show('pair');
  try {
    const response = await fetch(`/api/check?token=${encodeURIComponent(token)}`);
    if (response.status === 401) { storage.set(TOKEN, null); return show('pair'); }
  } catch {  }
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
    storage.set(TOKEN, body.token);
    show('remote');
    connect();
  } catch { $('pair-error').textContent = 'Cannot reach PALTO. Is this device on the same network?'; }
});

function connect() {
  const token = storage.get(TOKEN) || '';
  const scheme = location.protocol === 'https:' ? 'wss' : 'ws';
  socket = new WebSocket(`${scheme}://${location.host}/ws?token=${encodeURIComponent(token)}`);
  setStatus('Connecting…', false);
  socket.addEventListener('open', () => { retry = 0; setStatus('Connected', true); keepAwake(); });
  socket.addEventListener('message', message => receive(JSON.parse(message.data)));
  socket.addEventListener('close', () => {
    setStatus('Reconnecting…', false);
    socket = null;
    setTimeout(check, Math.min(10000, 500 * 2 ** retry++));
  });
}

async function keepAwake() {
  try { await navigator.wakeLock?.request('screen'); } catch {  }
}
document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'visible' && socket) keepAwake(); });

function setStatus(text, ok) {
  $('connection').textContent = text;
  $('connection').classList.toggle('ok', ok);
  document.querySelectorAll('.controls button, .media button').forEach(b => { b.disabled = !ok; });
}

function send(type, payload) {
  if (socket?.readyState !== WebSocket.OPEN) return;
  socket.send(JSON.stringify(payload === undefined ? { type } : { type, payload }));
  if (navigator.vibrate) navigator.vibrate(12);
}

function receive(event) {
  if (event.type === 'service_changed') state.live = event.payload.service;
  else if (event.type === 'program_changed') state.program = event.payload.program_state;
  else if (event.type === 'playback_changed') { state.playback = event.payload.playback; state.playbackAt = Date.now(); }
  else if (event.type === 'error' && event.payload.source === 'remote') { flash(event.payload.message); return; }
  else return;
  render();
}

function flash(message) {
  $('error').textContent = message;
  setTimeout(() => { if ($('error').textContent === message) $('error').textContent = ''; }, 3000);
}

function splitSlides(lyrics) {
  const slides = [];
  let current = [];
  for (const line of lyrics.replace(/\r\n?/g, '\n').split('\n')) {
    if (!line.trim()) { if (current.length) { slides.push(current.join('\n')); current = []; } }
    else current.push(line.trim());
  }
  if (current.length) slides.push(current.join('\n'));
  return slides;
}

function sectionInfo(section) {
  const [prefix, color, label] = SECTIONS[section.kind] || SECTIONS.other;
  const code = (section.kind === 'other' ? (section.name || '').trim().charAt(0).toUpperCase() || 'P' : prefix) + (section.number ?? '');
  const name = section.kind === 'other' ? (section.name || label) : `${label}${section.number != null ? ` ${section.number}` : ''}`;
  return { code, color: section.color || color, name };
}

function slidesOf(item) {
  const kind = item?.kind;
  if (!kind) return [];
  if (kind.kind === 'text') {
    if (item.song?.sections?.length) {
      return item.song.sections.flatMap(section => {
        const info = sectionInfo(section);
        return splitSlides(section.lyrics).map(text => ({ text, ...info }));
      });
    }
    return kind.data.steps.map((text, i) => ({ text, code: String(i + 1), color: null, name: '' }));
  }
  if (kind.kind === 'presentation') return (kind.data.slides ?? []).map((_, i) => ({ text: `Slide ${i + 1}`, code: String(i + 1), color: null, name: '' }));
  return [];
}

const items = () => state.live?.items ?? [];
const titleOf = item => item?.title || item?.song?.title || KINDS[item?.kind?.kind] || '';
const cueable = item => item && item.kind.kind !== 'section';

function render() {
  const service = state.live?.service;
  $('service').textContent = service?.title ?? 'No service loaded';
  $('service-date').textContent = service?.scheduledAtMs ? new Date(service.scheduledAtMs).toLocaleDateString(undefined, { weekday: 'long', month: 'short', day: 'numeric' }) : '';
  const display = state.program?.display ?? 'live';
  document.querySelectorAll('button[data-display]').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.display === display)));
  $('display-banner').hidden = display === 'live';
  $('display-banner').textContent = { black: 'Projector is black', clear: 'Words are cleared', logo: 'Projector shows the logo' }[display] ?? '';
  renderLive();
  renderOrder();
  renderMedia();
}

function renderLive() {
  const list = items();
  const index = list.findIndex(item => item.id === state.program?.active_item_id);
  const item = list[index];
  const step = state.program?.active_step_index ?? 0;
  const slides = slidesOf(item);
  const slide = slides[step];
  setScreen('now', item ? [titleOf(item), slide?.name].filter(Boolean).join(' · ') : 'Now', slide?.color, item ? (state.program.active_text || slide?.text || titleOf(item)) : 'Nothing on screen', !item);
  $('now-count').textContent = slides.length > 1 ? `${step + 1} / ${slides.length}` : '';
  let nextLabel = 'Next';
  let nextText = '';
  let nextColor = null;
  if (item && step + 1 < slides.length) {
    const upcoming = slides[step + 1];
    nextLabel = upcoming.name ? `Next · ${upcoming.name}` : 'Next';
    nextText = upcoming.text;
    nextColor = upcoming.color;
  } else {
    const following = list.slice(index + 1).find(cueable);
    if (following) {
      const first = slidesOf(following)[0];
      nextLabel = `Next · ${titleOf(following)}`;
      nextText = first?.text || KINDS[following.kind.kind] || '';
      nextColor = first?.color;
    }
  }
  setScreen('next', nextLabel, nextColor, nextText || (item ? 'End of service' : ''), !nextText);
}

function setScreen(which, label, color, text, quiet) {
  $(`${which}-label`).textContent = label;
  $(`${which}-dot`).hidden = !color;
  $(`${which}-dot`).style.background = color || '';
  $(`${which}-text`).textContent = text;
  $(`${which}-text`).classList.toggle('quiet', quiet);
}

function renderOrder() {
  const activeId = state.program?.active_item_id;
  const step = state.program?.active_step_index ?? 0;
  const order = $('order');
  const scroll = $('view-order').scrollTop;
  order.replaceChildren(...items().map(item => {
    if (item.kind.kind === 'section') return el('li', { class: 'heading' }, titleOf(item));
    const current = item.id === activeId;
    const open = state.open.has(item.id) || (current && !state.open.has(`closed:${item.id}`));
    const slides = slidesOf(item);
    const codes = item.song?.sections?.map(sectionInfo) ?? [];
    const toggle = () => {
      if (open) { state.open.delete(item.id); if (current) state.open.add(`closed:${item.id}`); }
      else { state.open.add(item.id); state.open.delete(`closed:${item.id}`); }
      renderOrder();
    };
    return el('li', { class: `item${current ? ' current' : ''}${open ? ' open' : ''}` },
      el('button', { 'aria-expanded': String(open), onclick: toggle },
        el('span', { class: 'item-main' },
          el('span', {}, titleOf(item)),
          codes.length ? el('span', { class: 'chips' }, codes.map(c => el('span', { class: 'chip', style: `--c:${c.color}` }, c.code))) : el('small', { class: 'kind' }, KINDS[item.kind.kind] ?? '')),
        current && el('span', { class: 'live-tag' }, 'LIVE'),
        el('span', { class: 'chevron', 'aria-hidden': 'true' }, '›')),
      open && el('ol', { class: 'slides' }, slides.length
        ? slides.map((slide, i) => el('li', {}, el('button', { class: current && i === step ? 'on' : '', style: slide.color ? `--c:${slide.color}` : '', onclick: () => send('cue', { item_id: item.id, index: i }) },
          el('span', { class: 'code' }, slide.code), el('span', { class: 'text' }, slide.text))))
        : el('li', {}, el('button', { class: `show${current ? ' on' : ''}`, onclick: () => send('cue', { item_id: item.id, index: 0 }) }, current ? 'On screen' : `Show ${KINDS[item.kind.kind]?.toLowerCase() ?? 'item'}`))));
  }));
  if (!items().length) order.append(el('li', { class: 'heading' }, 'No service loaded in PALTO'));
  $('view-order').scrollTop = scroll;
}

function position() {
  const playback = state.playback;
  if (!playback) return 0;
  const running = !playback.paused && !playback.suspended;
  const elapsed = running ? (Date.now() - state.playbackAt) * (playback.rate_percent || 100) / 100 : 0;
  return Math.min(playback.position_ms + elapsed, playback.duration_ms ?? Infinity);
}

const clock = ms => { const s = Math.max(0, Math.floor(ms / 1000)); return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };

function renderMedia() {
  const playback = state.playback;
  $('media').hidden = !playback;
  if (!playback) return;
  const item = items().find(i => i.id === playback.item_id);
  $('media-title').textContent = titleOf(item) || (playback.kind === 'audio' ? 'Audio' : 'Video');
  const at = position();
  $('media-time').textContent = playback.duration_ms ? `${clock(at)} / ${clock(playback.duration_ms)}` : clock(at);
  $('media-progress').style.width = playback.duration_ms ? `${Math.min(100, at / playback.duration_ms * 100)}%` : '0';
  const playing = !playback.paused;
  $('media-toggle').textContent = playing ? 'Pause' : 'Play';
  $('media-toggle').setAttribute('aria-pressed', String(playing));
}
setInterval(() => { if (state.playback && !$('media').hidden) renderMedia(); }, 500);

function setTab(tab) {
  state.tab = tab;
  storage.set(TAB, tab);
  document.querySelectorAll('[data-tab]').forEach(b => b.setAttribute('aria-selected', String(b.dataset.tab === tab)));
  $('view-live').hidden = tab !== 'live';
  $('view-order').hidden = tab !== 'order';
  if (tab === 'order') requestAnimationFrame(() => document.querySelector('.item.current')?.scrollIntoView({ block: 'center' }));
}

document.querySelectorAll('[data-tab]').forEach(button => button.addEventListener('click', () => setTab(button.dataset.tab)));
document.querySelectorAll('button[data-display]').forEach(button => button.addEventListener('click', () => send('set_display', { display: button.dataset.display })));
$('prev').addEventListener('click', () => send('prev'));
$('next-button').addEventListener('click', () => send('next'));
$('media-toggle').addEventListener('click', () => send(state.playback && !state.playback.paused ? 'pause' : 'play'));
$('media-stop').addEventListener('click', () => send('stop'));
document.addEventListener('keydown', event => {
  if ($('remote').hidden || event.target instanceof HTMLInputElement) return;
  if (event.key === 'ArrowRight' || event.key === ' ' || event.key === 'PageDown') { event.preventDefault(); send('next'); }
  if (event.key === 'ArrowLeft' || event.key === 'PageUp') { event.preventDefault(); send('prev'); }
});

setTab(storage.get(TAB) === 'order' ? 'order' : 'live');
setStatus('Connecting…', false);
check();
