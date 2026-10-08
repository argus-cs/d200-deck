import { invoke } from '@tauri-apps/api/core';
import type { KeyFace, LabelStyle, Screen } from './types';

// Previews come from the engine, so they are exactly what the device shows.
// Each one is rendered once and kept.
const urls = $state<Record<string, string | null>>({});
const pending = new Set<string>();

function cached(id: string, render: () => Promise<string | null>): string | null | undefined {
  if (!(id in urls) && !pending.has(id)) {
    pending.add(id);
    render()
      .then((url) => (urls[id] = url))
      .catch(() => (urls[id] = null))
      .finally(() => pending.delete(id));
  }
  return urls[id];
}

/** An icon on a background, as the icon picker shows it. */
export function iconUrl(icon: string | null | undefined, color: string, iconColor?: string | null): string | null {
  const id = `icon|${icon ?? ''}|${color}|${iconColor ?? ''}`;
  return cached(id, () => invoke<string>('render_icon', { icon: icon ?? null, color, iconColor: iconColor ?? null })) ?? null;
}

/** A whole key: colors, frame and label, as on the device. */
export function keyUrl(face: KeyFace, label: LabelStyle): string | null {
  const look = {
    label: face.label,
    icon: face.icon ?? null,
    color: face.color,
    icon_color: face.icon_color ?? null,
    text_color: face.text_color ?? null,
    border: face.border ?? null,
  };
  const style = { show: label.show, size: label.size, color: label.color, align: label.align };
  return cached(`key|${JSON.stringify(look)}|${JSON.stringify(style)}`, () => invoke<string>('render_key', { key: look, label: style })) ?? null;
}

/** `undefined` while drawing, `null` when the device draws this visor itself. */
export function screenUrl(screen: Screen): string | null | undefined {
  // What a tap or a hold does doesn't change the picture.
  const { action: _action, hold: _hold, ...look } = screen;
  return cached(`screen|${JSON.stringify(look)}`, () => invoke<string | null>('render_screen', { screen: look }));
}
