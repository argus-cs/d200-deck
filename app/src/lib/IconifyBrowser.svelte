<script lang="ts">
  import { onMount } from 'svelte';
  import { iconifyIcons, iconifySamples, iconifySave, iconifySearch, iconifySets, type IconSet, type IconifyIcon } from './api';
  import Icon from './Icon.svelte';
  import { ICON_COLOR, fold } from './types';

  let {
    color,
    iconColor = null,
    onpick,
    onclose,
  }: {
    /** The key's background, so icons are previewed as they will look. */
    color: string;
    iconColor?: string | null;
    /** The saved icon's path, to store on the key. */
    onpick: (path: string) => void;
    onclose: () => void;
  } = $props();

  const PAGE = 150;
  const LAST_SET = 'd200deck.iconifySet';
  /** Samples asked for at once (three per set). */
  const SAMPLE_BATCH = 60;

  let dialog: HTMLDialogElement;
  let sentinel = $state<HTMLElement | null>(null);

  let sets = $state<IconSet[]>([]);
  let setsQuery = $state('');
  let current = $state<IconSet | null>(null);
  let filter = $state('');
  let icons = $state<IconifyIcon[]>([]);
  let total = $state(0);
  /** Search results across every set, named "mdi:bluetooth", instead of one set's icons. */
  let results = $state<IconifyIcon[] | null>(null);
  let searching = $state(false);
  /** The sets' samples by id ("mdi:home"), as they scroll into view. */
  let sampleSvg = $state<Record<string, string>>({});
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let pasted = $state('');
  /** Answers to an older filter or set arrive late: only the latest counts. */
  let request = 0;
  let filterTimer: ReturnType<typeof setTimeout> | undefined;

  const ink = $derived(iconColor ?? ICON_COLOR);
  const groups = $derived.by(() => {
    const q = fold(setsQuery.trim());
    const shown = q ? sets.filter((s) => fold(`${s.name} ${s.prefix} ${s.category}`).includes(q)) : sets;
    const out: { category: string; sets: IconSet[] }[] = [];
    for (const set of shown) {
      const last = out[out.length - 1];
      if (last?.category === set.category) last.sets.push(set);
      else out.push({ category: set.category || 'Outras', sets: [set] });
    }
    return out;
  });

  /** Icons come as SVG text from the engine: one-color ones in the key's icon color. */
  const local = (svg: string) => `data:image/svg+xml,${encodeURIComponent(svg.replaceAll('currentColor', ink))}`;

  // Samples are asked for as their sets scroll into view, one request at a
  // time: the engine gets them from Iconify (or its disk) one set per
  // request, as Iconify turns away many single icons from one address.
  const asked = new Set<string>();
  let waiting: string[] = [];
  let askingSamples = false;
  const setOf = new WeakMap<Element, IconSet>();
  const shown = new IntersectionObserver((entries) => {
    for (const entry of entries) {
      const set = setOf.get(entry.target);
      if (!entry.isIntersecting || !set) continue;
      for (const sample of set.samples) {
        const id = `${set.prefix}:${sample}`;
        if (!asked.has(id)) {
          asked.add(id);
          waiting.push(id);
        }
      }
    }
    askSamples();
  });

  /** Watches a set's row, for its samples. */
  function sampled(node: HTMLElement, set: IconSet) {
    setOf.set(node, set);
    shown.observe(node);
    return {
      update: (next: IconSet) => setOf.set(node, next),
      destroy: () => shown.unobserve(node),
    };
  }

  async function askSamples() {
    if (askingSamples || waiting.length === 0) return;
    askingSamples = true;
    const ids = waiting.splice(0, SAMPLE_BATCH);
    try {
      const found = await iconifySamples(ids);
      for (const icon of found) sampleSvg[icon.name] = icon.svg;
      // The ones missing are asked again when their set comes back into view.
      const got = new Set(found.map((icon) => icon.name));
      for (const id of ids) if (!got.has(id)) asked.delete(id);
    } catch {
      for (const id of ids) asked.delete(id);
    } finally {
      askingSamples = false;
      askSamples();
    }
  }

  function readLast() {
    try {
      return localStorage.getItem(LAST_SET);
    } catch {
      return null;
    }
  }

  function rememberLast(prefix: string) {
    try {
      localStorage.setItem(LAST_SET, prefix);
    } catch {
      // Only remembered while the window is open then.
    }
  }

  // Loads the next page when the end of the grid comes into view.
  const observer = new IntersectionObserver((entries) => {
    if (entries.some((e) => e.isIntersecting)) more();
  });
  $effect(() => {
    const end = sentinel;
    if (!end) return;
    observer.observe(end);
    return () => observer.unobserve(end);
  });

  onMount(() => {
    dialog.showModal();
    iconifySets()
      .then((list) => {
        sets = list;
        const last = list.find((s) => s.prefix === readLast());
        if (last) openSet(last);
      })
      .catch((e) => (error = `Não deu para falar com o Iconify: ${e}`));
    return () => {
      observer.disconnect();
      shown.disconnect();
      clearTimeout(filterTimer);
    };
  });

  async function load(offset: number) {
    if (!current) return;
    const id = ++request;
    loading = true;
    searching = false;
    error = null;
    try {
      const page = await iconifyIcons(current.prefix, filter, offset, PAGE);
      if (id !== request) return;
      icons = offset === 0 ? page.icons : [...icons, ...page.icons];
      total = page.total;
    } catch (e) {
      if (id === request) error = String(e);
    } finally {
      if (id === request) loading = false;
    }
  }

  function more() {
    if (!loading && !results && current && icons.length < total) load(icons.length);
  }

  function openSet(set: IconSet) {
    current = set;
    results = null;
    filter = '';
    icons = [];
    total = 0;
    rememberLast(set.prefix);
    load(0);
  }

  function onFilter() {
    results = null;
    searching = false;
    clearTimeout(filterTimer);
    filterTimer = setTimeout(() => load(0), 200);
  }

  async function searchAll() {
    const query = filter.trim();
    if (!query) return;
    const id = ++request;
    loading = true;
    searching = true;
    error = null;
    try {
      const found = await iconifySearch(query);
      if (id === request) results = found;
    } catch (e) {
      if (id === request) error = String(e);
    } finally {
      if (id === request) {
        loading = false;
        searching = false;
      }
    }
  }

  async function pick(id: string) {
    saving = true;
    error = null;
    try {
      onpick(await iconifySave(id.trim()));
      dialog.close();
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<dialog bind:this={dialog} class="browser" aria-label="Mais ícones" {onclose}>
  <header>
    <div>
      <h2>Mais ícones</h2>
      <p class="muted small">Do Iconify, a mesma fonte do Icônes. Cada coleção é baixada na primeira vez e fica guardada no PC.</p>
    </div>
    <button type="button" class="icon-btn" aria-label="Fechar" title="Fechar (Esc)" onclick={() => dialog.close()}>
      <Icon name="close" size={16} />
    </button>
  </header>

  <div class="body">
    <aside>
      <div class="search">
        <Icon name="search" size={16} />
        <input type="text" aria-label="Buscar coleção" placeholder={`Buscar entre ${sets.length || ''} coleções`} bind:value={setsQuery} />
      </div>
      <ul class="sets scroll">
        {#each groups as group (group.category)}
          <li class="category">{group.category}</li>
          {#each group.sets as set (set.prefix)}
            <li>
              <button type="button" class="set" class:on={current?.prefix === set.prefix} onclick={() => openSet(set)} use:sampled={set}>
                <span class="set-top">
                  <span class="ellipsis">{set.name}</span>
                  <span class="samples">
                    {#each set.samples as sample (sample)}
                      {@const svg = sampleSvg[`${set.prefix}:${sample}`]}
                      {#if svg}<img src={local(svg)} alt="" />{:else}<span class="blank"></span>{/if}
                    {/each}
                  </span>
                </span>
                <span class="muted small">{set.total.toLocaleString('pt-BR')} ícones · {set.license}</span>
              </button>
            </li>
          {/each}
        {/each}
      </ul>
    </aside>

    <section>
      <div class="tools">
        <div class="search grow">
          <Icon name="search" size={16} />
          <input
            type="text"
            aria-label="Filtrar ícones"
            placeholder={current ? `Filtrar em ${current.name}` : 'Escolha uma coleção ou busque em todas'}
            bind:value={filter}
            oninput={onFilter}
            onkeydown={(e) => {
              if (e.key === 'Enter') searchAll();
            }}
          />
        </div>
        <button type="button" class="secondary" disabled={!filter.trim() || loading} onclick={searchAll}>Buscar em todas</button>
      </div>

      {#if searching}
        <div class="empty muted">Buscando "{filter.trim()}" em todas as coleções…</div>
      {:else if results}
        <p class="muted small">
          {results.length === 0 ? 'Nada encontrado em nenhuma coleção.' : `${results.length} ícones de várias coleções para "${filter.trim()}".`}
          {#if current}<button type="button" class="link" onclick={() => openSet(current!)}>Voltar para {current.name}</button>{/if}
        </p>
        <div class="grid scroll" style:--tile={color}>
          {#each results as icon (icon.name)}
            <button type="button" class="tile" title={icon.name} aria-label={`Ícone ${icon.name}`} disabled={saving} onclick={() => pick(icon.name)}>
              <img src={local(icon.svg)} alt="" />
            </button>
          {/each}
        </div>
      {:else if current}
        <p class="muted small">
          {current.name} · {current.author || 'autor desconhecido'} · {current.license}{current.attribution ? ' (pede crédito ao autor)' : ''}{current.palette
            ? ' · ícones coloridos: a cor do ícone da tecla não muda eles'
            : ''}
        </p>
        <div class="grid scroll" style:--tile={color}>
          {#each icons as icon (icon.name)}
            <button
              type="button"
              class="tile"
              title={icon.name}
              aria-label={`Ícone ${icon.name}`}
              disabled={saving}
              onclick={() => pick(`${current!.prefix}:${icon.name}`)}
            >
              <img src={local(icon.svg)} alt="" />
            </button>
          {/each}
          <div class="sentinel" bind:this={sentinel}></div>
        </div>
        <p class="muted small">
          {#if loading && icons.length === 0}
            Baixando a coleção…
          {:else if total === 0 && !loading}
            Nenhum ícone com esse nome nesta coleção.
          {:else}
            Mostrando {icons.length.toLocaleString('pt-BR')} de {total.toLocaleString('pt-BR')}{loading ? '…' : ''}
          {/if}
        </p>
      {:else}
        <div class="empty muted">Escolha uma coleção à esquerda, ou digite acima e busque em todas.</div>
      {/if}
      {#if error}<p class="error-text small">{error}</p>{/if}
    </section>
  </div>

  <footer>
    <label for="iconify-id" class="small">Já copiou o nome no Icônes (icones.js.org)?</label>
    <input
      id="iconify-id"
      class="mono"
      type="text"
      placeholder="mdi:bluetooth"
      bind:value={pasted}
      onkeydown={(e) => {
        if (e.key === 'Enter' && pasted.trim()) pick(pasted);
      }}
    />
    <button type="button" class="primary" disabled={!pasted.trim() || saving} onclick={() => pick(pasted)}>Usar</button>
    {#if saving}<span class="muted small">Salvando…</span>{/if}
  </footer>
</dialog>

<style>
  .browser {
    width: min(980px, 92vw);
    height: min(700px, 88vh);
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--panel);
    color: var(--text);
    overflow: hidden;
  }
  .browser[open] {
    display: flex;
    flex-direction: column;
  }
  .browser::backdrop {
    background: rgb(0 0 0 / 0.45);
  }
  header {
    flex: none;
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 16px 20px 12px;
    border-bottom: 1px solid var(--line-soft);
  }
  h2 {
    font-size: 18px;
    font-weight: 600;
  }
  .icon-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 9px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--soft);
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }
  aside {
    flex: none;
    width: 300px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-right: 1px solid var(--line-soft);
  }
  section {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px;
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
    min-width: 0;
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
  .grow {
    flex: 1 1 auto;
  }
  .tools {
    display: flex;
    gap: 8px;
  }
  .sets {
    flex: 1 1 auto;
    min-height: 0;
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .category {
    padding: 10px 4px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .set {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    padding: 7px 9px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: transparent;
    text-align: left;
  }
  .set:hover {
    background: var(--hover);
  }
  .set.on {
    border-color: var(--accent);
  }
  .set-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    min-width: 0;
  }
  .samples {
    flex: none;
    display: flex;
    gap: 4px;
  }
  .samples img,
  .samples .blank {
    width: 18px;
    height: 18px;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grid {
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(56px, 1fr));
    grid-auto-rows: max-content;
    gap: 6px;
    padding: 2px;
  }
  .tile {
    aspect-ratio: 1 / 1;
    padding: 12px;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: var(--tile);
  }
  .tile:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .tile img {
    width: 100%;
    height: 100%;
    display: block;
  }
  .sentinel {
    grid-column: 1 / -1;
    height: 1px;
  }
  .empty {
    flex: 1 1 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
  }
  .link {
    border: 0;
    padding: 0 4px;
    background: transparent;
    color: var(--accent-text);
    text-decoration: underline;
  }
  footer {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--line-soft);
  }
  footer input {
    width: 220px;
  }
</style>
