<script lang="ts">
  import { keyUrl, screenUrl } from './icons.svelte';
  import { KEY_NUMBERS, screenLabel, type Key, type LabelStyle, type Screen } from './types';

  let {
    keys,
    inherited = null,
    label,
    screen,
    screenState,
    selected,
    onselect,
    onswap,
  }: {
    keys: Record<string, Key>;
    /** The default layout under a rule; null when editing the default layout itself. */
    inherited?: Record<string, Key> | null;
    /** Text style, since labels are drawn into the key images. */
    label: LabelStyle;
    /** The visor this layer shows. */
    screen: Screen;
    /** 'override': a rule's own visor; 'inherited': the default one under a rule. */
    screenState: 'own' | 'override' | 'inherited';
    selected: number;
    onselect: (n: number) => void;
    /** A key was dragged onto another: swap the two in this layer. */
    onswap: (from: number, to: number) => void;
  } = $props();

  let dragging = $state<number | null>(null);
  let over = $state<number | null>(null);

  const tiles = $derived(
    KEY_NUMBERS.filter((n) => n <= 13).map((n) => {
      const own = keys[n];
      const base = inherited?.[n];
      const shown = own ?? base ?? null;
      return {
        n,
        shown,
        own: !!own,
        twoState: !!shown?.toggle,
        image: shown ? keyUrl(shown, label) : null,
        state: own ? (inherited ? 'override' : 'own') : base ? 'inherited' : 'empty',
      };
    }),
  );
  const screenImage = $derived(screenUrl(screen));

  function drop(to: number) {
    if (dragging !== null && dragging !== to) onswap(dragging, to);
    dragging = null;
    over = null;
  }
</script>

<div class="deck">
  {#each tiles as tile (tile.n)}
    <button
      type="button"
      class="key {tile.state}"
      class:selected={tile.n === selected}
      class:dragging={dragging === tile.n}
      class:over={over === tile.n && dragging !== tile.n}
      draggable={tile.own}
      aria-pressed={tile.n === selected}
      aria-label={`Tecla ${tile.n}: ${tile.shown?.label || 'vazia'}${tile.state === 'inherited' ? ' (do padrão)' : ''}`}
      title={tile.own ? 'Arraste sobre outra tecla para trocar as duas de lugar' : undefined}
      onclick={() => onselect(tile.n)}
      ondragstart={(e) => {
        dragging = tile.n;
        e.dataTransfer?.setData('text/plain', String(tile.n));
        if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
      }}
      ondragend={() => {
        dragging = null;
        over = null;
      }}
      ondragover={(e) => {
        if (dragging === null) return;
        e.preventDefault();
        over = tile.n;
      }}
      ondragleave={() => {
        if (over === tile.n) over = null;
      }}
      ondrop={(e) => {
        e.preventDefault();
        drop(tile.n);
      }}
    >
      {#if tile.image}<img src={tile.image} alt="" draggable="false" />{/if}
      {#if !tile.shown}<span class="hint">+</span>{/if}
      {#if tile.twoState}<span class="two" title="Tecla de dois estados">2</span>{/if}
    </button>
  {/each}
  <button
    type="button"
    class="key screen {screenState}"
    class:selected={selected === 14}
    aria-pressed={selected === 14}
    aria-label={`Visor: ${screenLabel(screen.content.type)}${screenState === 'inherited' ? ' (do padrão)' : ''}`}
    onclick={() => onselect(14)}
  >
    {#if screenImage}
      <img src={screenImage} alt="" draggable="false" />
    {:else}
      <span class="hint small-hint">{screenLabel(screen.content.type)}</span>
    {/if}
  </button>
</div>

<style>
  .deck {
    align-self: center;
    width: 100%;
    max-width: 640px;
    flex: none;
    padding: 18px;
    background: var(--device);
    border: 1px solid var(--device-line);
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
    border: 1px solid var(--device-key-line);
    background: var(--device-key);
    color: var(--device-text);
    overflow: hidden;
  }
  .key[draggable='true'] {
    cursor: grab;
  }
  .screen {
    grid-column: span 2;
    aspect-ratio: auto;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .screen img {
    object-fit: fill;
  }
  img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    pointer-events: none;
  }
  .hint {
    color: var(--device-muted);
    font-size: 18px;
  }
  .small-hint {
    font-size: 13px;
    padding: 0 8px;
  }
  .two {
    position: absolute;
    top: 6px;
    left: 6px;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--accent);
    color: var(--ink);
    font-size: 10px;
    font-weight: 700;
    line-height: 16px;
    pointer-events: none;
  }
  .empty {
    background: var(--device-empty);
    border-style: dashed;
    border-color: #4a4e57;
  }
  .inherited {
    border-style: dashed;
    border-color: #4a4e57;
  }
  .inherited img {
    opacity: 0.35;
  }
  .override {
    border: 2px solid var(--accent);
  }
  .selected {
    box-shadow: 0 0 0 3px var(--device), 0 0 0 5px var(--device-text);
  }
  .dragging {
    opacity: 0.4;
  }
  .over {
    box-shadow: 0 0 0 3px var(--device), 0 0 0 5px var(--accent);
    transform: scale(1.04);
  }
</style>
