<script lang="ts">
  import type { KeyStatus, ScreenStatus } from './types';

  let {
    keys,
    selected,
    screen: visor,
    lit = [],
    onselect,
  }: {
    keys: KeyStatus[];
    selected: number;
    screen: ScreenStatus;
    /** Keys just pressed on the device, to light up. */
    lit?: number[];
    onselect: (n: number) => void;
  } = $props();

  const buttons = $derived(keys.filter((k) => k.number <= 13));

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
      class:lit={lit.includes(key.number)}
      class:empty={!key.image && !key.label}
      aria-label={`Tecla ${key.number}: ${key.label || 'vazia'}`}
      aria-pressed={key.number === selected}
      onclick={() => onselect(key.number)}
    >
      {#if key.image}<img src={key.image} alt="" />{/if}
      {#if key.rule}<span class="dot" aria-hidden="true"></span>{/if}
    </button>
  {/each}
  <button
    type="button"
    class="key screen"
    class:from-rule={!!visor.rule}
    class:selected={selected === 14}
    class:lit={lit.includes(14)}
    aria-label="Visor (tecla 14)"
    aria-pressed={selected === 14}
    onclick={() => onselect(14)}
  >
    {#if visor.image}
      <img class="visor" src={visor.image} alt="" />
    {:else if visor.content === 'device_stats'}
      <span class="stats">CPU · RAM</span>
    {:else}
      <span class="time">{time}</span>
      <span class="day">{day}</span>
    {/if}
    {#if visor.rule}<span class="dot" aria-hidden="true"></span>{/if}
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
    overflow: hidden;
    color: var(--device-text);
  }
  .key img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  /* The device stretches the visor image to its panel; so does this. */
  .key img.visor {
    object-fit: fill;
  }
  .empty {
    background: var(--device-empty);
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
    box-shadow: 0 0 0 2px var(--device);
  }
  .selected {
    box-shadow: 0 0 0 3px var(--device), 0 0 0 5px var(--device-text);
  }
  .lit {
    animation: press 450ms ease-out;
  }
  @keyframes press {
    0% {
      transform: scale(0.92);
      box-shadow: 0 0 0 3px var(--device), 0 0 0 6px var(--accent), 0 0 24px 4px var(--accent);
    }
    100% {
      transform: scale(1);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .lit {
      animation: none;
      box-shadow: 0 0 0 3px var(--device), 0 0 0 6px var(--accent);
    }
  }
  .screen {
    grid-column: span 2;
    aspect-ratio: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    background: var(--device-empty);
  }
  .time {
    font-family: var(--mono);
    font-size: 28px;
    font-weight: 500;
  }
  .day,
  .stats {
    font-size: 12px;
    color: var(--device-muted);
  }
</style>
