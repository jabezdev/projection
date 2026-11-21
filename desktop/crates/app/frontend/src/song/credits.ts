import type { Author, AuthorRole } from '../api/models';

const roles: Record<string, AuthorRole> = { words: 'words', lyrics: 'words', music: 'music', translation: 'translation' };
export const authorsText = (authors: Author[] = []) => authors.map(a => a.role ? `${a.name} (${a.role})` : a.name).join(', ');
export function parseAuthors(text: string): Author[] {
  return text.split(',').map(part => part.trim()).filter(Boolean).map(part => {
    const match = /^(.*?)\s*\((\w+)\)$/.exec(part);
    const role = match ? roles[match[2].toLowerCase()] : undefined;
    return role ? { name: match![1], role } : { name: part };
  });
}
