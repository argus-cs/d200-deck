<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    value,
    label,
    onchange,
    allowNone = false,
    noneLabel = 'Nenhuma',
    placeholder,
  }: {
    value: string | null | undefined;
    label: string;
    onchange: (color: string | null) => void;
    /** Offers going back to no color (the default, or no frame). */
    allowNone?: boolean;
    noneLabel?: string;
    /** The color that applies when there is none, shown in the swatch. */
    placeholder?: string;
  } = $props();

  const GROUPS: { name: string; colors: string[] }[] = [
    {
      name: 'Escuros (bons para fundo)',
      colors: ['#111214', '#24262B', '#3A3D44', '#1F2A44', '#12303A', '#1F3A2A', '#3A2A12', '#3A1616', '#2A1F3A', '#3A1F33'],
    },
    {
      name: 'Vivos',
      colors: ['#F0A63A', '#E5533D', '#E0457B', '#9B59D0', '#4C6FFF', '#2D9CDB', '#1FB5A0', '#3DBE5A', '#A3C93A', '#F2C94C'],
    },
    {
      name: 'Claros (bons para texto e ícone)',
      colors: ['#FFFFFF', '#ECEDEF', '#C9CBD0', '#8A8E96', '#FFD9A8', '#FFC2B8', '#FFC9E0', '#C9D8FF', '#BFF0D4', '#FFF1B8'],
    },
  ];
  const RECENT = 'd200deck.recentColors';
  const canPickFromScreen = typeof window !== 'undefined' && 'EyeDropper' in window;

  let open = $state(false);
  let root: HTMLDivElement | undefined = $state();
  let recent = $state<string[]>(readRecent());
  let typed = $state('');

  $effect(() => {
    typed = value ?? '';
  });

  function readRecent(): string[] {
    try {
      return JSON.parse(localStorage.getItem(RECENT) ?? '[]');
    } catch {
      return [];
    }
  }

  function remember(color: string) {
    recent = [color, ...recent.filter((c) => c !== color)].slice(0, 10);
    try {
      localStorage.setItem(RECENT, JSON.stringify(recent));
    } catch {
      // Only remembered while the window is open then.
    }
  }

  function choose(color: string | null, keep = true) {
    const normalized = color ? color.toUpperCase() : null;
    onchange(normalized);
    if (normalized && keep) remember(normalized);
  }

  function onTyped(text: string) {
    typed = text;
    const match = text.trim().match(/^#?([0-9a-fA-F]{6})$/);
    if (match) choose(`#${match[1]}`);
  }

  async function pickFromScreen() {
    try {
      // Chromium's EyeDropper: the person clicks anywhere on the screen.
      const result = await new (window as unknown as { EyeDropper: new () => { open(): Promise<{ sRGBHex: string }> } }).EyeDropper().open();
      choose(result.sRGBHex);
    } catch {
      // Cancelled with Esc.
    }
  }

  const same = (color: string) => !!value && value.toUpperCase() === color.toUpperCase();
</script>

<svelte:window
  onclick={(e) => {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }}
  onkeydown={(e) => {
    if (e.key === 'Escape') open = false;
  }}
/>

<div class="color" bind:this={root}>
  <button type="button" class="trigger" aria-expanded={open} onclick={() => (open = !open)}>
    <span class="chip" class:none={!value && !placeholder} style:background={value ?? placeholder ?? 'transparent'}></span>
    <span class="name">{label}</span>
    <span class="mono muted small">{value ?? noneLabel}</span>
  </button>

  {#if open}
    <div class="panel" role="dialog" aria-label={`Escolher a cor: ${label}`}>
      {#each GROUPS as group (group.name)}
        <span class="group">{group.name}</span>
        <div class="grid">
          {#each group.colors as color (color)}
            <button
              type="button"
              class="swatch"
              style:background={color}
              title={color}
              aria-label={color}
              aria-pressed={same(color)}
              onclick={() => choose(color)}
            ></button>
          {/each}
        </div>
      {/each}

      {#if recent.length > 0}
        <span class="group">Usadas recentemente</span>
        <div class="grid">
          {#each recent as color (color)}
            <button
              type="button"
              class="swatch"
              style:background={color}
              title={color}
              aria-label={color}
              aria-pressed={same(color)}
              onclick={() => choose(color)}
            ></button>
          {/each}
        </div>
      {/if}

      <div class="row">
        <input class="mono hex" type="text" aria-label="Código da cor" placeholder="#RRGGBB" value={typed} oninput={(e) => onTyped(e.currentTarget.value)} />
        <input
          type="color"
          aria-label="Escolher no seletor do Windows"
          value={value ?? placeholder ?? '#000000'}
          oninput={(e) => choose(e.currentTarget.value, false)}
          onchange={(e) => choose(e.currentTarget.value)}
        />
        {#if canPickFromScreen}
          <button type="button" class="icon-btn" title="Pegar uma cor de qualquer lugar da tela" aria-label="Conta-gotas" onclick={pickFromScreen}>
            <Icon name="eyedropper" size={16} />
          </button>
        {/if}
      </div>
      {#if allowNone}
        <button type="button" class="secondary small-btn" aria-pressed={!value} onclick={() => choose(null)}>{noneLabel}</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .color {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .trigger {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 10px 6px 6px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--surface);
    text-align: left;
  }
  .trigger:hover {
    background: var(--hover);
  }
  .trigger[aria-expanded='true'] {
    border-color: var(--accent);
  }
  .chip {
    width: 26px;
    height: 26px;
    flex: none;
    border-radius: 7px;
    border: 1px solid var(--line-strong);
  }
  .chip.none {
    background: repeating-linear-gradient(45deg, transparent 0 4px, var(--line) 4px 6px) !important;
  }
  .name {
    flex: 1 1 auto;
    font-weight: 500;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--line-soft);
    background: var(--raised);
  }
  .group {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(10, minmax(0, 1fr));
    gap: 5px;
  }
  .swatch {
    aspect-ratio: 1 / 1;
    padding: 0;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
  }
  .swatch:hover {
    transform: scale(1.12);
  }
  .swatch[aria-pressed='true'] {
    box-shadow: 0 0 0 2px var(--raised), 0 0 0 4px var(--accent);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .hex {
    flex: 1 1 auto;
    min-width: 0;
    text-transform: uppercase;
  }
  input[type='color'] {
    width: 38px;
    height: 34px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: transparent;
    flex: none;
  }
  .icon-btn {
    width: 34px;
    height: 34px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 8px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--soft);
  }
  .icon-btn:hover {
    background: var(--hover);
  }
  .small-btn {
    padding: 6px 10px;
    font-size: 13px;
    align-self: flex-start;
  }
  .small-btn[aria-pressed='true'] {
    border-color: var(--accent);
  }
</style>
