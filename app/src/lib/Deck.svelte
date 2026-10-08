<script lang="ts">
  import type { KeyStatus, Status } from './types';

  let {
    keys,
    selected,
    window: screenMode,
    onselect,
  }: { keys: KeyStatus[]; selected: number; window: Status['window']; onselect: (n: number) => void } = $props();

  const buttons = $derived(keys.filter((k) => k.number <= 13));
  const screen = $derived(keys.find((k) => k.number === 14));

  let now = $state(new Date());
  $effect(() => {
    const timer = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(timer);
  });
  const time = $derived(now.toLocaleTimeString('pt-BR', { hour: '2-digit', minute: '2-digit' }));
  const day = $derived(now.toLocaleDateString('pt-BR', { weekday: 'short', day: 'numeric', month: 'short' }));
</script>

<div class="deck">
  {#each buttons as key (key.number)}
    <button
      type="button"
      class="key"
      class:from-rule={!!key.rule}
      class:selected={key.number === selected}
      class:empty={!key.image && !key.label}
      aria-label={`Tecla ${key.number}: ${key.label || 'vazia'}`}
      aria-pressed={key.number === selected}
      onclick={() => onselect(key.number)}
    >
      {#if key.image}<img src={key.image} alt="" />{/if}
      {#if key.label}<span class="label">{key.label}</span>{/if}
      {#if key.rule}<span class="dot" aria-hidden="true"></span>{/if}
    </button>
  {/each}
  <button
    type="button"
    class="key screen"
    class:from-rule={!!screen?.rule}
    class:selected={selected === 14}
    aria-label="Visor (tecla 14)"
    aria-pressed={selected === 14}
    onclick={() => onselect(14)}
  >
    {#if screenMode === 'image' && screen?.image}
      <img src={screen.image} alt="" />
    {:else if screenMode === 'stats'}
      <span class="stats">CPU · RAM</span>
    {:else}
      <span class="time">{time}</span>
      <span class="day">{day}</span>
    {/if}
  </button>
</div>

<style>
  .deck {
    align-self: center;
    width: 100%;
    max-width: 640px;
    box-sizing: border-box;
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
    color: var(--text);
  }
  .key img {
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
    line-height: 1.2;
    text-align: center;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.8);
  }
  .empty {
    background: var(--panel);
    border-style: dashed;
    border-color: #4a4e57;
  }
  .from-rule {
    border: 2px solid var(--accent);
  }
  .dot {
    position: absolute;
    top: 7px;
    right: 7px;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 0 2px #0a0a0c;
  }
  .selected {
    box-shadow: 0 0 0 3px #0a0a0c, 0 0 0 5px var(--text);
  }
  .screen {
    grid-column: span 2;
    aspect-ratio: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    background: var(--panel);
  }
  .time {
    font-family: var(--mono);
    font-size: 28px;
    font-weight: 500;
  }
  .day,
  .stats {
    font-size: 12px;
    color: var(--muted);
  }
</style>
