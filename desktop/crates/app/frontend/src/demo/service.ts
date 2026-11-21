// Browser-only demo content: a service with one of every item type, so the
// whole operator UI can be seen and restyled without the desktop app. The
// media lives in frontend/demo/ and is served at /demo/ by the dev server only.
import { defaultCaptureOptions, defaultVideoOptions, type CaptureSource, type Media, type ProjectModel, type SessionItem } from '../api/models';
import type { SongDocument } from '../song/song';

const song = (parts: SongDocument['parts'], arrangement: string[], background = ''): SongDocument =>
  ({ format: 'magi.song.v1', slideSeparator: 'blank-line', background, parts, arrangement });

const faithfulness = song([
  { id: 'v1', type: 'Verse', number: 1, lyrics: 'Great is Thy faithfulness, O God my Father\nThere is no shadow of turning with Thee\n\nThou changest not, Thy compassions they fail not\nAs Thou hast been, Thou forever wilt be' },
  { id: 'c1', type: 'Chorus', number: 1, lyrics: 'Great is Thy faithfulness\nGreat is Thy faithfulness\n\nMorning by morning new mercies I see\nAll I have needed Thy hand hath provided\n\nGreat is Thy faithfulness\nLord, unto me' },
  { id: 'v2', type: 'Verse', number: 2, lyrics: 'Summer and winter and springtime and harvest\nSun, moon and stars in their courses above\n\nJoin with all nature in manifold witness\nTo Thy great faithfulness, mercy and love' },
], ['v1', 'c1', 'v2', 'c1'], '/demo/background-dawn.svg');

const vision = song([
  { id: 'v1', type: 'Verse', number: 1, lyrics: 'Be Thou my vision, O Lord of my heart\nNaught be all else to me, save that Thou art\n\nThou my best thought, by day or by night\nWaking or sleeping, Thy presence my light' },
  { id: 'v2', type: 'Verse', number: 2, lyrics: 'Be Thou my wisdom, and Thou my true word\nI ever with Thee and Thou with me, Lord' },
  { id: 'b1', type: 'Bridge', number: 1, lyrics: 'High King of heaven, my victory won\nMay I reach heaven’s joys, O bright heaven’s sun' },
], ['v1', 'v2', 'b1'], '/demo/worship-loop.mp4');

const amazing = song([
  { id: 'i1', type: 'Intro', number: 1, lyrics: '(Instrumental)' },
  { id: 'v1', type: 'Verse', number: 1, lyrics: 'Amazing grace, how sweet the sound\nThat saved a wretch like me\n\nI once was lost, but now am found\nWas blind, but now I see' },
  { id: 'p1', type: 'Pre-chorus', number: 1, lyrics: 'My chains are gone, I’ve been set free' },
  { id: 'c1', type: 'Chorus', number: 1, lyrics: 'My God, my Savior has ransomed me\n\nAnd like a flood His mercy reigns\nUnending love, amazing grace' },
  { id: 't1', type: 'Tag', number: 1, lyrics: 'Unending love, amazing grace' },
], ['i1', 'v1', 'p1', 'c1', 't1'], 'shader:aurora');

/** One step per slide, in arrangement order, as the engine cues them. */
const steps = (doc: SongDocument) => doc.arrangement.flatMap(id => {
  const part = doc.parts.find(p => p.id === id);
  return part ? part.lyrics.split(/\n[ \t]*\n/).map(s => s.trim()).filter(Boolean) : [];
});

const songItem = (id: number, title: string, doc: SongDocument, librarySongId: number): SessionItem =>
  ({ id, title, kind: { kind: 'text', data: { steps: steps(doc) } }, song: JSON.stringify({ ...doc, librarySongId }), background: doc.background || undefined });

const sermonSlides = [1, 2, 3, 4].map(n => `/demo/sermon-${n}.svg`);
const scriptureSlides = [1, 2].map(n => `/demo/scripture-${n}.svg`);

export const DEMO_PROJECT_ID = 1;

export const demoProject: ProjectModel = {
  id: DEMO_PROJECT_ID,
  name: 'Demo · Every item type',
  session: {
    items: [
      { id: 100, title: 'Welcome', kind: { kind: 'section', data: {} } },
      { id: 101, title: 'Welcome slide', kind: { kind: 'image', data: { path: '/demo/welcome.svg' } } },
      { id: 102, title: 'Countdown', kind: { kind: 'video', data: { path: '/demo/countdown.mp4', options: { ...defaultVideoOptions, confidence: true } } } },
      { id: 112, title: 'Praise & Worship', kind: { kind: 'section', data: {} } },
      songItem(103, 'Great Is Thy Faithfulness', faithfulness, 301),
      songItem(104, 'Be Thou My Vision', vision, 302),
      { id: 113, title: 'Message', kind: { kind: 'section', data: {} } },
      { id: 105, title: 'Scripture · Psalm 23', kind: { kind: 'presentation', data: { path: '/demo/scripture-1.svg', slides: scriptureSlides } } },
      { id: 106, title: 'Sermon slides (PDF)', kind: { kind: 'presentation', data: { path: 'Good Shepherd sermon.pdf', slides: sermonSlides } } },
      { id: 107, title: 'Sermon notes window', kind: { kind: 'capture', data: { source: { kind: 'window', id: 'demo:window-1', name: 'Sermon notes — PowerPoint' }, options: { ...defaultCaptureOptions, confidence: true } } } },
      { id: 114, title: 'Response', kind: { kind: 'section', data: {} } },
      { id: 108, title: 'Offering music', kind: { kind: 'audio', data: { path: '/demo/offering-music.mp3', options: { ...defaultVideoOptions, looping: true, volume: 80 } } } },
      songItem(109, 'Amazing Grace (My Chains Are Gone)', amazing, 303),
      { id: 110, title: 'Live stream feed', kind: { kind: 'capture', data: { source: { kind: 'screen', id: 'demo:screen-1', name: 'Display 1 (1920×1080)' }, options: { ...defaultCaptureOptions, projection_fit: 'fill' } } } },
      { id: 111, title: 'Closing loop', kind: { kind: 'video', data: { path: '/demo/worship-loop.mp4', options: { ...defaultVideoOptions, looping: true, volume: 0 } } } },
    ],
  },
};

export const demoMedia: Media[] = [
  { id: 201, title: 'Welcome slide', kind: 'image', path: '/demo/welcome.svg' },
  { id: 202, title: 'Dawn background', kind: 'image', path: '/demo/background-dawn.svg' },
  { id: 203, title: 'Church logo', kind: 'image', path: '/demo/church-logo.svg' },
  { id: 204, title: 'Countdown', kind: 'video', path: '/demo/countdown.mp4' },
  { id: 205, title: 'Worship loop', kind: 'video', path: '/demo/worship-loop.mp4' },
  { id: 206, title: 'Offering music', kind: 'audio', path: '/demo/offering-music.mp3' },
];

export const demoSongs: Record<string, { id: number; title: string; lyrics: string }> = Object.fromEntries(
  ([[301, 'Great Is Thy Faithfulness', faithfulness], [302, 'Be Thou My Vision', vision], [303, 'Amazing Grace (My Chains Are Gone)', amazing]] as const)
    .map(([id, title, doc]) => [String(id), { id, title, lyrics: JSON.stringify(doc) }]),
);

/** The logo the Logo key shows in the demo. */
export const DEMO_LOGO = '/demo/church-logo.svg';

/** Browser-only mode: screens and windows to pick when capturing. */
export const DEMO_SOURCES: CaptureSource[] = [
  { kind: 'screen', id: 'demo:screen-1', name: 'Display 1 (1920×1080)' },
  { kind: 'window', id: 'demo:window-1', name: 'Sermon notes — PowerPoint' },
];
