import { convertFileSrc } from '@tauri-apps/api/core';
import { isTauri } from '../api/tauri';
export function mediaSource(path: string) { return isTauri() ? convertFileSrc(path) : path; }
