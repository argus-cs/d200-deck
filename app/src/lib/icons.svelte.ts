import { invoke } from '@tauri-apps/api/core';

// Previews come from the engine, so they are exactly what the device shows.
// Each icon + color is rendered once and kept.
const urls = $state<Record<string, string | null>>({});
const pending = new Set<string>();

export function iconUrl(icon: string | null | undefined, color: string): string | null {
  const id = `${icon ?? ''}|${color}`;
  if (!(id in urls) && !pending.has(id)) {
    pending.add(id);
    invoke<string>('render_icon', { icon: icon ?? null, color })
      .then((url) => (urls[id] = url))
      .catch(() => (urls[id] = null))
      .finally(() => pending.delete(id));
  }
  return urls[id] ?? null;
}
