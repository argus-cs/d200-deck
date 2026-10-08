<script lang="ts">
  import { iconUrl } from './icons.svelte';
  import { KEY_NUMBERS, type Key } from './types';

  let {
    keys,
    inherited = null,
    selected,
    onselect,
  }: {
    keys: Record<string, Key>;
    /** The default layout under a rule; null when editing the default layout itself. */
    inherited?: Record<string, Key> | null;
    selected: number;
    onselect: (n: number) => void;
  } = $props();

  const tiles = $derived(
    KEY_NUMBERS.map((n) => {
      const own = keys[n];
      const base = inherited?.[n];
      const shown = own ?? base ?? null;
      return {
        n,
        shown,
        image: shown ? iconUrl(shown.icon, shown.color) : null,
        state: own ? (inherited ? 'override' : 'own') : base ? 'inherited' : 'empty',
      };
    }),
  );
</script>

<div class="deck">
  {#each tiles as tile (tile.n)}
    <button
      type="button"
      class="key {tile.state}"
      class:screen={tile.n === 14}
      class:selected={tile.n === selected}
      aria-pressed={tile.n === selected}
      aria-label={`Tecla ${tile.n}: ${tile.shown?.label || 'vazia'}${tile.state === 'inherited' ? ' (do padrão)' : ''}`}
      onclick={() => onselect(tile.n)}
    >
      {#if tile.image}<img src={tile.image} alt="" />{/if}
      {#if tile.shown?.label}<span class="label">{tile.shown.label}</span>{/if}
      {#if !tile.shown}<span class="hint">{tile.n === 14 ? 'Visor' : '+'}</span>{/if}
    </button>
  {/each}
</div>

<style>
  .deck {
    align-self: center;
    width: 100%;
    max-width: 640px;
    padding: 18px;
    background: #0a0a0c;
    border: 1px solid #2a2c31;
    border-radius: 24px;
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 12px;
  }
  .key {
    position: relative;
    aspect-ratio: 1 / 1;
    padding: 0;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--key);
    overflow: hidden;
  }
  .screen {
    grid-column: span 2;
    aspect-ratio: auto;
  }
  .screen img {
    object-fit: contain;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .label {
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 6px;
    font-size: 11px;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.8);
  }
  .hint {
    color: var(--muted);
    font-size: 18px;
  }
  .screen .hint {
    font-size: 13px;
  }
  .empty {
    background: var(--panel);
    border-style: dashed;
    border-color: #4a4e57;
  }
  .inherited {
    border-style: dashed;
    border-color: #4a4e57;
  }
  .inherited img,
  .inherited .label {
    opacity: 0.35;
  }
  .override {
    border: 2px solid var(--accent);
  }
  .selected {
    box-shadow: 0 0 0 3px #0a0a0c, 0 0 0 5px var(--text);
  }
</style>
