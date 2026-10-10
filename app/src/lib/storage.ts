// Small conveniences kept between sessions in this window's localStorage (the
// last Iconify set, recent colors, a dismissed guide). Reading or writing can
// fail, with site data blocked for instance: then nothing is kept, and the
// window works the same.

export function stored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** A stored JSON value, or `fallback` when there is none or it doesn't parse. */
export function storedJson<T>(key: string, fallback: T): T {
  try {
    return JSON.parse(localStorage.getItem(key) ?? '') as T;
  } catch {
    return fallback;
  }
}

export function store(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Kept only while the window is open, then.
  }
}
