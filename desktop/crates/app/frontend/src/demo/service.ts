import { defaultCaptureOptions, defaultVideoOptions, type CaptureSource, type MediaFile, type Section, type SectionKind, type Service, type ServiceItem, type Song } from '../api/models';

const part = (id: string, kind: SectionKind, number: number | undefined, lyrics: string): Section => ({ id, kind, number, lyrics });
const song = (id: string, title: string, sections: Section[], arrangement: string[], background?: string, ccli?: string): Song =>
  ({ id, title, archived: false, credits: ccli ? { ccli } : {}, tags: [], current: { id: `${id}-v1`, number: 1, sections, arrangement, background } });

export const demoMedia: MediaFile[] = [
  { id: 'm-welcome', title: 'Welcome slide', kind: 'image', path: '/demo/welcome.svg' },
  { id: 'm-dawn', title: 'Dawn background', kind: 'image', path: '/demo/background-dawn.svg' },
  { id: 'm-logo', title: 'Church logo', kind: 'image', path: '/demo/church-logo.svg' },
  { id: 'm-countdown', title: 'Countdown', kind: 'video', path: '/demo/countdown.mp4' },
  { id: 'm-loop', title: 'Worship loop', kind: 'video', path: '/demo/worship-loop.mp4' },
  { id: 'm-offering', title: 'Offering music', kind: 'audio', path: '/demo/offering-music.mp3' },
];

export const demoSongs: Song[] = [
  song('s-faithfulness', 'Great Is Thy Faithfulness', [
    part('v1', 'verse', 1, 'Great is Thy faithfulness, O God my Father\nThere is no shadow of turning with Thee\n\nThou changest not, Thy compassions they fail not\nAs Thou hast been, Thou forever wilt be'),
    part('c1', 'chorus', 1, 'Great is Thy faithfulness\nGreat is Thy faithfulness\n\nMorning by morning new mercies I see\nAll I have needed Thy hand hath provided\n\nGreat is Thy faithfulness\nLord, unto me'),
    part('v2', 'verse', 2, 'Summer and winter and springtime and harvest\nSun, moon and stars in their courses above\n\nJoin with all nature in manifold witness\nTo Thy great faithfulness, mercy and love'),
  ], ['v1', 'c1', 'v2', 'c1'], 'm-dawn', '18723'),
  song('s-vision', 'Be Thou My Vision', [
    part('v1', 'verse', 1, 'Be Thou my vision, O Lord of my heart\nNaught be all else to me, save that Thou art\n\nThou my best thought, by day or by night\nWaking or sleeping, Thy presence my light'),
    part('v2', 'verse', 2, 'Be Thou my wisdom, and Thou my true word\nI ever with Thee and Thou with me, Lord'),
    part('b1', 'bridge', 1, 'High King of heaven, my victory won\nMay I reach heaven’s joys, O bright heaven’s sun'),
  ], ['v1', 'v2', 'b1'], 'm-loop'),
  song('s-amazing', 'Amazing Grace (My Chains Are Gone)', [
    part('i1', 'intro', undefined, '(Instrumental)'),
    part('v1', 'verse', 1, 'Amazing grace, how sweet the sound\nThat saved a wretch like me\n\nI once was lost, but now am found\nWas blind, but now I see'),
    part('p1', 'pre_chorus', undefined, 'My chains are gone, I’ve been set free'),
    part('c1', 'chorus', undefined, 'My God, my Savior has ransomed me\n\nAnd like a flood His mercy reigns\nUnending love, amazing grace'),
    part('t1', 'tag', undefined, 'Unending love, amazing grace'),
  ], ['i1', 'v1', 'p1', 'c1', 't1'], 'shader:aurora', '4768151'),
];

const sermonSlides = [1, 2, 3, 4].map(n => `/demo/sermon-${n}.svg`);
const scriptureSlides = [1, 2].map(n => `/demo/scripture-${n}.svg`);
const item = (id: string, title: string | undefined, content: ServiceItem['content']): ServiceItem => ({ id, ...(title ? { title } : {}), content });

export const DEMO_SERVICE_ID = 'demo-service';

export function demoService(): Service {
  const at = new Date(); at.setHours(10, 0, 0, 0);
  return {
    id: DEMO_SERVICE_ID,
    title: 'Demo · Every item type',
    scheduledAtMs: at.getTime(),
    items: [
      item('i100', 'Welcome', { kind: 'header' }),
      item('i101', 'Welcome slide', { kind: 'media', mediaId: 'm-welcome', options: defaultVideoOptions }),
      item('i102', 'Countdown', { kind: 'media', mediaId: 'm-countdown', options: { ...defaultVideoOptions, confidence: true } }),
      item('i112', 'Praise & Worship', { kind: 'header' }),
      item('i103', undefined, { kind: 'song', songId: 's-faithfulness' }),
      item('i104', undefined, { kind: 'song', songId: 's-vision' }),
      item('i113', 'Message', { kind: 'header' }),
      item('i105', 'Scripture · Psalm 23', { kind: 'presentation', slides: scriptureSlides }),
      item('i106', 'Sermon slides (PDF)', { kind: 'presentation', slides: sermonSlides }),
      item('i107', 'Sermon notes window', { kind: 'capture', source: { kind: 'window', id: 'demo:window-1', name: 'Sermon notes — PowerPoint' }, options: { ...defaultCaptureOptions, confidence: true } }),
      item('i114', 'Response', { kind: 'header' }),
      item('i108', 'Offering music', { kind: 'media', mediaId: 'm-offering', options: { ...defaultVideoOptions, looping: true, volume: 80 } }),
      item('i109', undefined, { kind: 'song', songId: 's-amazing' }),
      item('i110', 'Live stream feed', { kind: 'capture', source: { kind: 'screen', id: 'demo:screen-1', name: 'Display 1 (1920×1080)' }, options: { ...defaultCaptureOptions, projection_fit: 'fill' } }),
      item('i111', 'Closing loop', { kind: 'media', mediaId: 'm-loop', options: { ...defaultVideoOptions, looping: true, volume: 0 } }),
    ],
  };
}

export const DEMO_LOGO = '/demo/church-logo.svg';

export const DEMO_SOURCES: CaptureSource[] = [
  { kind: 'screen', id: 'demo:screen-1', name: 'Display 1 (1920×1080)' },
  { kind: 'window', id: 'demo:window-1', name: 'Sermon notes — PowerPoint' },
];
