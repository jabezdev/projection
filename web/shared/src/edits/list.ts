import { same } from "./json";

type Keyed = { id: string };

export type ListChange<T> =
  | { kind: "insert"; value: T; prev: string | null }
  | { kind: "remove"; value: T; prev: string | null }
  | { kind: "move"; id: string; from: string | null; to: string | null };

export function diffList<T extends Keyed>(before: T[], after: T[]): ListChange<T>[] {
  const work = [...before];
  const changes: ListChange<T>[] = [];
  const wanted = new Set(after.map((v) => v.id));
  for (let i = 0; i < work.length; ) {
    if (wanted.has(work[i].id)) { i++; continue; }
    const prev = i > 0 ? work[i - 1].id : null;
    changes.push({ kind: "remove", value: work.splice(i, 1)[0], prev });
  }
  const positions = new Map(work.map((v, i) => [v.id, i]));
  const kept = stable(after.filter((v) => positions.has(v.id)).map((v) => [v.id, positions.get(v.id)!] as [string, number]));
  after.forEach((value, i) => {
    const to = i > 0 ? after[i - 1].id : null;
    if (!positions.has(value.id)) {
      insert(work, value, to);
      changes.push({ kind: "insert", value, prev: to });
    } else if (!kept.has(value.id)) {
      const from = predecessor(work, value.id);
      moveTo(work, value.id, to);
      changes.push({ kind: "move", id: value.id, from, to });
    }
  });
  return changes;
}

function stable(sequence: [string, number][]): Set<string> {
  const tails: number[] = [];
  const links: (number | null)[] = sequence.map(() => null);
  sequence.forEach(([, value], i) => {
    let at = 0;
    while (at < tails.length && sequence[tails[at]][1] < value) at++;
    links[i] = at > 0 ? tails[at - 1] : null;
    if (at === tails.length) tails.push(i); else tails[at] = i;
  });
  const kept = new Set<string>();
  for (let cursor: number | null = tails.length ? tails[tails.length - 1] : null; cursor !== null; cursor = links[cursor]) kept.add(sequence[cursor][0]);
  return kept;
}

export const index = <T extends Keyed>(list: T[], id: string) => list.findIndex((v) => v.id === id);

export function predecessor<T extends Keyed>(list: T[], id: string): string | null {
  const i = index(list, id);
  return i > 0 ? list[i - 1].id : null;
}

export function insert<T extends Keyed>(list: T[], value: T, prev: string | null): boolean {
  if (index(list, value.id) >= 0) return false;
  if (prev === null) { list.unshift(value); return true; }
  const p = index(list, prev);
  if (p < 0) { list.push(value); return false; }
  list.splice(p + 1, 0, value);
  return true;
}

export function remove<T extends Keyed>(list: T[], value: T): boolean {
  const i = index(list, value.id);
  if (i < 0) return false;
  return same(list.splice(i, 1)[0], value);
}

export function moveTo<T extends Keyed>(list: T[], id: string, to: string | null): boolean {
  const i = index(list, id);
  if (i < 0) return false;
  return insert(list, list.splice(i, 1)[0], to);
}
