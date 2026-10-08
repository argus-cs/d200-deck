<script lang="ts">
  import { appIcon, pickAppIcon, pickImage, runningApps, siteIcon } from './api';
  import Icon from './Icon.svelte';
  import { iconUrl } from './icons.svelte';
  import { fold, type EdgePage, type Glyph, type RunningApp } from './types';

  let {
    icon,
    color,
    iconColor = null,
    glyphs,
    edgePages,
    onchange,
  }: {
    icon: string | null | undefined;
    color: string;
    /** Built-in icons are previewed in this color. */
    iconColor?: string | null;
    glyphs: Glyph[];
    edgePages: EdgePage[];
    onchange: (icon: string | null) => void;
  } = $props();

  let query = $state('');
  let panel = $state<'none' | 'app' | 'site'>('none');
  let apps = $state<RunningApp[]>([]);
  let busy = $state(false);
  let error = $state<string | null>(null);

  const found = $derived(query.trim() ? glyphs.filter((g) => fold(`${g.label} ${g.id}`).includes(fold(query.trim()))) : glyphs);
  const custom = $derived(!!icon && !glyphs.some((g) => g.id === icon));

  /** Runs a chooser that returns the icon path to store, or null when cancelled. */
  async function choose(task: () => Promise<string | null>) {
    busy = true;
    error = null;
    try {
      const path = await task();
      if (path) {
        onchange(path);
        panel = 'none';
      }
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function togglePanel(next: 'app' | 'site') {
    error = null;
    panel = panel === next ? 'none' : next;
    if (panel === 'app') apps = (await runningApps()).filter((a) => a.exe.toLowerCase() !== 'd200-deck.exe');
  }
</script>

<div class="picker">
  <div class="search">
    <Icon name="search" size={16} />
    <input type="text" aria-label="Buscar ícone" placeholder={`Buscar entre ${glyphs.length} ícones`} bind:value={query} />
  </div>
  <div class="glyphs scroll">
    {#each found as glyph (glyph.id)}
      {@const url = iconUrl(glyph.id, color, iconColor)}
      <button
        type="button"
        class="glyph"
        class:on={icon === glyph.id}
        title={glyph.label}
        aria-label={`Ícone: ${glyph.label}`}
        aria-pressed={icon === glyph.id}
        onclick={() => onchange(glyph.id)}
      >
        {#if url}<img src={url} alt="" />{/if}
      </button>
    {/each}
    {#if found.length === 0}<p class="muted small none">Nenhum ícone com esse nome.</p>{/if}
  </div>

  <div class="row">
    <button type="button" class="secondary small-btn" aria-expanded={panel === 'app'} onclick={() => togglePanel('app')}>
      Ícone de app…
    </button>
    <button type="button" class="secondary small-btn" aria-expanded={panel === 'site'} onclick={() => togglePanel('site')}>
      Ícone de site…
    </button>
    <button type="button" class="secondary small-btn" onclick={() => choose(pickImage)}>Imagem PNG…</button>
    <button type="button" class="secondary small-btn" aria-pressed={!icon} onclick={() => onchange(null)}>Só a cor</button>
  </div>

  {#if panel === 'app'}
    <div class="panel">
      <span class="muted small">O ícone do próprio programa, como o Windows mostra.</span>
      <ul class="scroll">
        {#each apps as app (app.path)}
          <li>
            <button type="button" class="choice" disabled={busy} onclick={() => choose(() => appIcon(app.path))}>
              <span class="ellipsis">{app.title}</span>
              <span class="mono muted small">{app.exe}</span>
            </button>
          </li>
        {/each}
      </ul>
      <button type="button" class="secondary small-btn" disabled={busy} onclick={() => choose(pickAppIcon)}>Escolher um .exe…</button>
    </div>
  {:else if panel === 'site'}
    <div class="panel">
      {#if edgePages.length === 0}
        <span class="muted small">Abra o site no Edge (com a extensão conectada) e ele aparece aqui.</span>
      {:else}
        <span class="muted small">O ícone que o Edge já guarda para o site; nada é baixado.</span>
        <ul class="scroll">
          {#each edgePages as page (page.host)}
            <li>
              <button type="button" class="choice" disabled={busy} onclick={() => choose(() => siteIcon(page.url, page.host))}>
                <span class="ellipsis">{page.title || page.host}</span>
                <span class="mono muted small">{page.host}</span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  {/if}

  {#if busy}<span class="muted small">Buscando o ícone…</span>{/if}
  {#if error}<span class="error-text small">{error}</span>{/if}
  {#if custom}
    {@const url = iconUrl(icon, color, iconColor)}
    <div class="current">
      {#if url}<img src={url} alt="Ícone atual" />{/if}
      <span class="mono muted small ellipsis">{icon}</span>
    </div>
  {/if}
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted);
  }
  .search input {
    flex: 1 1 auto;
    border: 0;
    padding: 8px 0;
    background: transparent;
  }
  .search input:focus-visible {
    outline: none;
  }
  .search:focus-within {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .glyphs {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    grid-auto-rows: max-content;
    gap: 6px;
    max-height: 236px;
    padding: 2px;
  }
  .none {
    grid-column: 1 / -1;
  }
  .glyph {
    aspect-ratio: 1 / 1;
    padding: 0;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: var(--device-key);
    overflow: hidden;
  }
  .glyph img {
    width: 100%;
    height: 100%;
    display: block;
  }
  .glyph.on {
    border: 2px solid var(--accent);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .small-btn {
    padding: 6px 10px;
    font-size: 13px;
  }
  .small-btn[aria-pressed='true'],
  .small-btn[aria-expanded='true'] {
    border-color: var(--accent);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    background: var(--surface);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 200px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .choice {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 7px 9px;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: transparent;
    text-align: left;
    min-width: 0;
  }
  .choice:hover:not(:disabled) {
    background: var(--hover);
  }
  .choice span {
    max-width: 100%;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .current {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .current img {
    width: 44px;
    height: 44px;
    border-radius: 8px;
    flex: none;
  }
</style>
