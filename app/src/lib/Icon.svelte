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
  };

  let { name, size = 18 }: { name: string; size?: number } = $props();
</script>

<svg class="ico" viewBox="0 0 24 24" width={size} height={size} aria-hidden="true"><path d={PATHS[name]} /></svg>
