import { convertFileSrc } from '@tauri-apps/api/core';
import { isTauri } from './tauri';
export function mediaSource(path: string) { return isTauri() ? convertFileSrc(path) : path; }
