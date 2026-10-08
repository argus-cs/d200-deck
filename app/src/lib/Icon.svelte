<script lang="ts">
  // 24×24 stroke icons, same family as the key glyphs.
  const PATHS: Record<string, string> = {
    grid: 'M4 4h4v4H4z M10 4h4v4h-4z M16 4h4v4h-4z M4 10h4v4H4z M10 10h4v4h-4z M16 10h4v4h-4z M4 16h4v4H4z M10 16h10v4H10z',
    live: 'M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z M6.3 6.3a8 8 0 0 0 0 11.4 M17.7 6.3a8 8 0 0 1 0 11.4',
    app: 'M3 5h18v14H3z M3 9h18',
    site: 'M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18',
    pause: 'M8 5v14 M16 5v14',
    play: 'M8 5l11 7-11 7z',
    refresh: 'M20 12a8 8 0 1 1-2.3-5.7 M20 4v4h-4',
    file: 'M6 3h8l4 4v14H6z M14 3v4h4',
    plus: 'M12 5v14 M5 12h14',
    alert: 'M12 3l10 18H2z M12 10v4 M12 17.5v.5',
    undo: 'M9 14L4 9l5-5 M4 9h11a5 5 0 0 1 0 10h-3',
    redo: 'M15 14l5-5-5-5 M20 9H9a5 5 0 0 0 0 10h3',
    check: 'M5 12l5 5 9-10',
    copy: 'M8 8h12v12H8z M4 16V4h12',
    paste: 'M8 4h8v3H8z M6 5H5v16h14V5h-1',
    settings: 'M4 6h9 M17 6h3 M15 4v4 M4 12h3 M11 12h9 M9 10v4 M4 18h11 M19 18h1 M17 16v4',
    search: 'M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4',
    puzzle: 'M8 4h3a2 2 0 1 1 4 0h3v4a2 2 0 1 1 0 4v4h-4a2 2 0 1 0-4 0H6v-4a2 2 0 1 0 0-4V4z',
    grip: 'M9 6h.01 M15 6h.01 M9 12h.01 M15 12h.01 M9 18h.01 M15 18h.01',
    minimize: 'M5 12h14',
    maximize: 'M6 6h12v12H6z',
    restore: 'M8 9h10v10H8z M6 15V5h10',
    close: 'M6 6l12 12 M18 6L6 18',
    up: 'M6 15l6-6 6 6',
    down: 'M6 9l6 6 6-6',
    eyedropper: 'M14.5 6.5l3 3 M16 3l5 5-2.5 2.5-5-5z M14.5 6.5L5 16l-1 4 4-1 9.5-9.5',
  };

  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg class="ico" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true"><path d={PATHS[name]} /></svg>
