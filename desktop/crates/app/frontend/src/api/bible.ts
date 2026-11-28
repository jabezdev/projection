import { useEffect, useState } from 'react';
import { invoke } from './tauri';
import type { BibleHit, BibleInfo, Passage } from './models';

const PREFERENCE_KEY = 'palto.bible';
export type BiblePreference = { translation: string; parallel: string };

export function readPreference(): BiblePreference {
  try {
    const stored = JSON.parse(localStorage.getItem(PREFERENCE_KEY) ?? '{}') as Partial<BiblePreference>;
    return { translation: stored.translation || 'kjv', parallel: stored.parallel ?? '' };
  } catch {
    return { translation: 'kjv', parallel: '' };
  }
}

export function writePreference(preference: BiblePreference) {
  try { localStorage.setItem(PREFERENCE_KEY, JSON.stringify(preference)); } catch { return; }
}

export function useBibles(revision = 0) {
  const [bibles, setBibles] = useState<BibleInfo[]>([]);
  useEffect(() => {
    let disposed = false;
    void invoke<BibleInfo[]>('list_bibles').then(list => { if (!disposed) setBibles(list ?? []); }).catch(() => {});
    return () => { disposed = true; };
  }, [revision]);
  return bibles;
}

export function usePassage(reference: string, translation: string, parallel: string) {
  const key = reference.trim() ? JSON.stringify([reference, translation, parallel]) : '';
  const [passage, setPassage] = useState<{ key: string; value: Passage } | null>(null);
  useEffect(() => {
    if (!key) return;
    let disposed = false;
    const timer = window.setTimeout(() => {
      void invoke<Passage>('bible_passage', { reference, translation, parallel: parallel || null })
        .then(found => { if (!disposed) setPassage({ key, value: found }); })
        .catch(e => { if (!disposed) setPassage({ key, value: { title: reference, warning: String(e), slides: [] } }); });
    }, 180);
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [key, reference, translation, parallel]);
  return key && passage ? passage.value : null;
}

export function useBibleSearch(query: string, translation: string) {
  const key = query.trim().length >= 3 ? JSON.stringify([query.trim(), translation]) : '';
  const [hits, setHits] = useState<{ key: string; value: BibleHit[] } | null>(null);
  useEffect(() => {
    if (!key) return;
    let disposed = false;
    const timer = window.setTimeout(() => {
      void invoke<BibleHit[]>('search_bible', { query, translation }).then(found => { if (!disposed) setHits({ key, value: found ?? [] }); }).catch(() => { if (!disposed) setHits({ key, value: [] }); });
    }, 250);
    return () => { disposed = true; window.clearTimeout(timer); };
  }, [key, query, translation]);
  return key && hits?.key === key ? hits.value : null;
}
