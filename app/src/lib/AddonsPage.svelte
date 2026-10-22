<script lang="ts">
  import { onMount } from 'svelte';
  import { newerVersion } from './addons.svelte';
  import {
    addonCatalog,
    installAddon,
    openAddonPage,
    openAddonsFolder,
    openAddonsGuide,
    openLog,
    removeAddon,
    rescanAddons,
    retryAddon,
  } from './api';
  import Icon from './Icon.svelte';
  import SearchBox from './SearchBox.svelte';
  import { store, stored, storedJson } from './storage';
  import { fold, type AddonState, type AddonStatus, type Catalog, type CatalogAddon, type Config } from './types';

  let { config = $bindable(), list, configPath }: { config: Config; list: AddonStatus[]; configPath: string } = $props();

  const TRUSTED = 'trustedAddons';
  const TAB = 'addonsTab';
  const STATE: Record<AddonState, string> = {
    off: 'Desligado',
    running: 'Rodando',
    restarting: 'Abrindo de novo',
    stopped: 'Parou',
    invalid: 'Com erro',
  };

  type Tab = 'installed' | 'catalog';
  let tab = $state<Tab>(stored(TAB) === 'catalog' ? 'catalog' : 'installed');
  let query = $state('');
  /** Addons the person already agreed to run: switching them on asks nothing. */
  let trusted = $state<string[]>(storedJson<string[]>(TRUSTED, []));
  /** The id of the addon waiting for a yes. */
  let confirming = $state<string | null>(null);
  /** The folder of the addon waiting for a yes to be removed. */
  let removing = $state<string | null>(null);
  let failure = $state<string | null>(null);

  let catalog = $state<Catalog | null>(null);
  let catalogError = $state<string | null>(null);
  let loadingCatalog = $state(false);
  /** Addons downloading (by id) or being removed (by folder). */
  let busy = $state<string[]>([]);
  /** What went wrong installing or removing one, by id or folder. */
  let problems = $state<Record<string, string>>({});

  const folder = $derived(configPath.replace(/[^\\/]*$/, 'addons'));
  /** On as the editors have it (the status follows once the engine applies the config). */
  const isOn = (addon: AddonStatus) => (config.addons ?? []).includes(addon.id);

  const words = $derived(fold(query).split(/\s+/).filter(Boolean));
  /** Every word of the search is in one of the texts. */
  const matches = (texts: (string | null | undefined)[]) => {
    const haystack = fold(texts.join(' '));
    return words.every((word) => haystack.includes(word));
  };
  const shownInstalled = $derived(
    list.filter((addon) => matches([addon.name, addon.folder, addon.id, addon.author, addon.description, ...addon.actions.map((a) => a.name)])),
  );
  const shownCatalog = $derived(
    (catalog?.addons ?? []).filter((entry) => matches([entry.name, entry.id, entry.author, entry.description, ...entry.actions, ...entry.tags])),
  );
  const entries = $derived(new Map((catalog?.addons ?? []).map((entry) => [entry.id, entry])));
  /** The installed addon with an id, if its addon.json loaded. */
  const installedOf = (id: string) => list.find((addon) => addon.id === id && addon.state !== 'invalid');
  /** The catalog's entry when it has a newer version of an addon the person installed. */
  function updateFor(addon: AddonStatus): CatalogAddon | null {
    const entry = entries.get(addon.id);
    return entry && !addon.bundled && entry.supported && newerVersion(entry.version, addon.version) ? entry : null;
  }
  const updates = $derived(list.filter((addon) => addon.state !== 'invalid' && updateFor(addon)).length);

  // Addons copied into the folder show up when the page opens; the catalog
  // (downloaded once a day) tells which have a newer version.
  onMount(() => {
    run(rescanAddons);
    loadCatalog(false);
  });

  async function run(work: () => Promise<unknown>) {
    failure = null;
    try {
      await work();
    } catch (e) {
      failure = String(e);
    }
  }

  function choose(next: Tab) {
    tab = next;
    store(TAB, next);
  }

  async function loadCatalog(refresh: boolean) {
    loadingCatalog = true;
    catalogError = null;
    try {
      catalog = await addonCatalog(refresh);
    } catch (e) {
      catalogError = String(e);
    } finally {
      loadingCatalog = false;
    }
  }

  /** Runs an install or a removal, keeping what went wrong under `key`. */
  async function job(key: string, work: () => Promise<unknown>): Promise<boolean> {
    busy = [...busy, key];
    delete problems[key];
    try {
      await work();
      return true;
    } catch (e) {
      problems[key] = String(e);
      return false;
    } finally {
      busy = busy.filter((other) => other !== key);
    }
  }

  async function install(entry: CatalogAddon) {
    const isNew = !installedOf(entry.id);
    if (!(await job(entry.id, () => installAddon(entry.id)))) return;
    // A new one arrives off: take the person to it, asking the first time.
    if (isNew) {
      query = '';
      choose('installed');
      if (!trusted.includes(entry.id)) confirming = entry.id;
    }
  }

  async function remove(addon: AddonStatus) {
    removing = null;
    await job(addon.folder, () => removeAddon(addon.folder));
  }

  function toggle(addon: AddonStatus) {
    if (isOn(addon)) {
      enable(addon.id, false);
    } else if (addon.bundled || trusted.includes(addon.id)) {
      enable(addon.id, true);
    } else {
      confirming = addon.id;
    }
  }

  function enable(id: string, on: boolean) {
    const ids = (config.addons ?? []).filter((other) => other !== id);
    if (on) ids.push(id);
    // None on is left out of config.json.
    if (ids.length > 0) config.addons = ids;
    else delete config.addons;
  }

  function trust(addon: AddonStatus) {
    trusted = [...trusted, addon.id];
    store(TRUSTED, JSON.stringify(trusted));
    confirming = null;
    enable(addon.id, true);
  }

  /** A box that opens below an addon, maybe out of sight. */
  function reveal(node: HTMLElement) {
    node.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  function needs(missing: string[]) {
    const names = missing.join(', ');
    return missing.length === 1
      ? `Precisa do ${names}, que não foi achado no PATH deste PC.`
      : `Precisa de ${names}, que não foram achados no PATH deste PC.`;
  }
</script>

<div class="page scroll">
  <div class="content">
    <div class="intro">
      <h1>Addons</h1>
      <p class="soft">
        Addons trazem ações novas para as teclas: jogos, apps e serviços que o D200 Deck não conhece. Cada addon é um programa que roda junto
        com o app e pode mudar o que a tecla mostra.
      </p>
    </div>

    <div class="tabs">
      <div class="segmented" role="group" aria-label="Addons">
        <button type="button" aria-pressed={tab === 'installed'} onclick={() => choose('installed')}>
          Instalados
          {#if updates > 0}<span class="count" title="Atualizações no catálogo">{updates}</span>{/if}
        </button>
        <button type="button" aria-pressed={tab === 'catalog'} onclick={() => choose('catalog')}>Catálogo</button>
      </div>
      <SearchBox bind:value={query} label="Buscar addons" placeholder={tab === 'catalog' ? 'Buscar no catálogo' : 'Buscar nos instalados'} grow />
    </div>

    <div class="toolbar">
      {#if tab === 'installed'}
        <button type="button" class="secondary with-icon" onclick={() => run(openAddonsFolder)}><Icon name="folder" size={16} />Abrir a pasta de addons</button>
        <button type="button" class="secondary with-icon" onclick={() => run(rescanAddons)}><Icon name="refresh" size={16} />Procurar de novo</button>
      {:else}
        <button type="button" class="secondary with-icon" disabled={loadingCatalog} onclick={() => loadCatalog(true)}>
          <Icon name="refresh" size={16} />{loadingCatalog ? 'Atualizando…' : 'Atualizar a lista'}
        </button>
      {/if}
      <button type="button" class="secondary with-icon" onclick={() => run(openAddonsGuide)}><Icon name="file" size={16} />Como fazer e publicar um addon</button>
      {#if tab === 'installed'}<span class="mono muted small path ellipsis" title={folder}>{folder}</span>{/if}
    </div>
    {#if failure}<p class="error-text small">{failure}</p>{/if}

    {#if tab === 'installed'}
      {#if list.length === 0}
        <div class="notice">
          <p>Nenhum addon instalado ainda.</p>
          <p class="muted small">
            Procure um no <button type="button" class="link" onclick={() => choose('catalog')}>catálogo</button>, ou copie a pasta de um addon
            (com um <span class="mono">addon.json</span> dentro) para a pasta de addons.
          </p>
        </div>
      {:else if shownInstalled.length === 0}
        <p class="muted">
          Nenhum addon instalado com “{query}”. <button type="button" class="link" onclick={() => choose('catalog')}>Buscar no catálogo</button>
        </p>
      {/if}

      {#each shownInstalled as addon (addon.folder)}
        {@const invalid = addon.state === 'invalid'}
        {@const on = !invalid && isOn(addon)}
        {@const isNew = !invalid && !on && !addon.bundled && !trusted.includes(addon.id)}
        {@const update = invalid ? null : updateFor(addon)}
        {@const page = entries.get(addon.id)?.homepage}
        {@const working = busy.includes(addon.folder) || busy.includes(addon.id)}
        <section class="addon" class:invalid class:asking={confirming === addon.id || removing === addon.folder}>
          <div class="head">
            <div class="tile" class:bad={invalid}><Icon name={invalid ? 'alert' : 'puzzle'} size={24} /></div>
            <div class="info">
              <div class="title">
                <h2 class:mono={invalid}>{invalid ? addon.folder : addon.name}</h2>
                {#if !invalid}
                  <span class="mono muted small">{addon.version}</span>
                  {#if addon.bundled}
                    <span class="muted small">vem com o D200 Deck</span>
                  {:else if addon.author}
                    <span class="muted small">{addon.author}</span>
                  {/if}
                {/if}
                {#if isNew}<span class="badge">Novo</span>{/if}
                {#if update}<span class="badge">Versão {update.version} no catálogo</span>{/if}
              </div>
              {#if invalid}
                <p class="error-text small">Não foi carregado: {addon.error}</p>
              {:else}
                {#if addon.description}<p class="soft">{addon.description}</p>{/if}
                {#if addon.actions.length > 0}
                  <div class="chips">{#each addon.actions as action (action.id)}<span class="chip">{action.name}</span>{/each}</div>
                {/if}
                {#if isNew}<p class="muted small">Chegou desligado: addons novos só rodam depois que você liga.</p>{/if}
              {/if}
              {#if update || page || !addon.bundled}
                <div class="buttons">
                  {#if update}
                    <button type="button" class="primary small-btn with-icon" disabled={working} onclick={() => install(update)}>
                      <Icon name="download" size={14} />{busy.includes(addon.id) ? 'Atualizando…' : `Atualizar para ${update.version}`}
                    </button>
                  {/if}
                  {#if page}
                    <button type="button" class="secondary small-btn with-icon" onclick={() => run(() => openAddonPage(addon.id))}>
                      <Icon name="link" size={14} />Página do addon
                    </button>
                  {/if}
                  {#if !addon.bundled}
                    <button type="button" class="secondary small-btn with-icon" disabled={working} onclick={() => (removing = addon.folder)}>
                      <Icon name="trash" size={14} />{busy.includes(addon.folder) ? 'Removendo…' : 'Remover'}
                    </button>
                  {/if}
                </div>
              {/if}
              {#if problems[addon.folder] ?? problems[addon.id]}<p class="error-text small">{problems[addon.folder] ?? problems[addon.id]}</p>{/if}
            </div>
            {#if !invalid}
              <div class="side">
                <span class="state {on ? addon.state : 'off'}"><span class="dot"></span>{STATE[on ? addon.state : 'off']}</span>
                <button
                  type="button"
                  class="switch"
                  role="switch"
                  aria-checked={on}
                  aria-label={`Ligar o ${addon.name}`}
                  onclick={() => toggle(addon)}
                >
                  <span></span>
                </button>
              </div>
            {/if}
          </div>

          {#if on && addon.error}
            <div class="problem small">
              <p>{addon.error}</p>
              <div class="buttons">
                {#if addon.state === 'stopped'}
                  <button type="button" class="secondary small-btn" onclick={() => run(() => retryAddon(addon.id))}>Tentar de novo</button>
                {/if}
                <button type="button" class="secondary small-btn" onclick={() => run(openLog)}>Abrir o log</button>
              </div>
            </div>
          {/if}

          {#if removing === addon.folder}
            <div class="confirm" {@attach reveal}>
              <h3><Icon name="trash" size={18} />Remover o {invalid ? addon.folder : addon.name}?</h3>
              <p>
                A pasta do addon é apagada. As teclas dele ficam escuras e guardam o que foi configurado, e os dados que ele guardou ficam, para
                quando você instalar de novo.
              </p>
              <div class="buttons">
                <button type="button" class="primary" onclick={() => remove(addon)}>Remover</button>
                <button type="button" class="secondary" onclick={() => (removing = null)}>Cancelar</button>
              </div>
            </div>
          {/if}

          {#if confirming === addon.id}
            <div class="confirm" {@attach reveal}>
              <h3><Icon name="alert" size={18} />Ligar o {addon.name}?</h3>
              <p>
                Um addon é um programa. Ligado, ele roda junto com o D200 Deck e tem as mesmas permissões que você no Windows: pode ler e mudar os
                seus arquivos, acessar a internet e apertar teclas. Ligue só addons de quem você confia.
              </p>
              <dl>
                <dt>Autor</dt>
                <dd>{addon.author || 'não informado'}</dd>
                <dt>Vai rodar</dt>
                <dd class="mono">{addon.run}</dd>
                <dt>Na pasta</dt>
                <dd class="mono">{addon.path}</dd>
                {#if addon.actions.length > 0}
                  <dt>Ações</dt>
                  <dd>{addon.actions.map((action) => action.name).join(', ')}</dd>
                {/if}
              </dl>
              <div class="buttons">
                <button type="button" class="primary" onclick={() => trust(addon)}>Ligar o addon</button>
                <button type="button" class="secondary" onclick={() => (confirming = null)}>Agora não</button>
                <span class="muted small">Só pergunta na primeira vez. Depois, a chave liga e desliga direto.</span>
              </div>
            </div>
          {/if}
        </section>
      {/each}

      {#if list.length > 0}
        <p class="muted small">
          Desligar ou remover um addon não apaga as teclas dele: elas escurecem e voltam a funcionar quando ele voltar. Atalhos e ações dos addons
          rodam com as suas permissões do Windows.
        </p>
      {/if}
    {:else}
      {#if catalog?.offline}
        <p class="muted small">Não deu para atualizar o catálogo agora; esta é a lista da última vez. ({catalog.offline})</p>
      {/if}
      {#if catalogError}
        <div class="notice">
          <p>Não deu para abrir o catálogo.</p>
          <p class="error-text small">{catalogError}</p>
        </div>
      {:else if !catalog}
        <p class="muted">Carregando o catálogo…</p>
      {:else if catalog.addons.length === 0}
        <div class="notice">
          <p>O catálogo ainda não tem addons.</p>
          <p class="muted small">O guia "Como fazer e publicar um addon" explica como pôr o seu aqui.</p>
        </div>
      {:else if shownCatalog.length === 0}
        <p class="muted">Nenhum addon do catálogo com “{query}”.</p>
      {/if}

      {#each shownCatalog as entry (entry.id)}
        {@const installed = installedOf(entry.id)}
        {@const update = !!installed && !installed.bundled && entry.supported && newerVersion(entry.version, installed.version)}
        {@const installing = busy.includes(entry.id)}
        <section class="addon">
          <div class="head">
            <div class="tile"><Icon name="puzzle" size={24} /></div>
            <div class="info">
              <div class="title">
                <h2>{entry.name}</h2>
                <span class="mono muted small">{entry.version}</span>
                {#if entry.author}<span class="muted small">{entry.author}</span>{/if}
              </div>
              {#if entry.description}<p class="soft">{entry.description}</p>{/if}
              {#if entry.actions.length > 0}
                <div class="chips">{#each entry.actions as name, i (i)}<span class="chip">{name}</span>{/each}</div>
              {/if}
              {#if entry.tags.length > 0}<p class="muted small">{entry.tags.join(' · ')}</p>{/if}
              {#if entry.missing.length > 0}<p class="warning small"><Icon name="alert" size={14} />{needs(entry.missing)}</p>{/if}
              {#if !entry.supported}
                <p class="muted small">Feito para uma versão mais nova do D200 Deck: atualize o app para instalar.</p>
              {/if}
              {#if entry.homepage}
                <div class="buttons">
                  <button type="button" class="secondary small-btn with-icon" onclick={() => run(() => openAddonPage(entry.id))}>
                    <Icon name="link" size={14} />Página do addon
                  </button>
                </div>
              {/if}
              {#if problems[entry.id]}<p class="error-text small">{problems[entry.id]}</p>{/if}
            </div>
            <div class="side column">
              {#if installed?.bundled}
                <span class="muted small">Vem com o D200 Deck</span>
              {:else if installed && update}
                <button type="button" class="primary with-icon" disabled={installing} onclick={() => install(entry)}>
                  <Icon name="download" size={16} />{installing ? 'Atualizando…' : `Atualizar para ${entry.version}`}
                </button>
                <span class="muted small">Você tem a {installed.version}</span>
              {:else if installed}
                <span class="installed small"><Icon name="check" size={16} />Instalado</span>
              {:else if !entry.supported}
                <button type="button" class="secondary" disabled>Precisa de um app mais novo</button>
              {:else}
                <button type="button" class="primary with-icon" disabled={installing} onclick={() => install(entry)}>
                  <Icon name="download" size={16} />{installing ? 'Instalando…' : 'Instalar'}
                </button>
              {/if}
            </div>
          </div>
        </section>
      {/each}

      {#if catalog && catalog.addons.length > 0}
        <p class="muted small">
          Os addons do catálogo são revisados antes de entrar, e o app confere se o arquivo baixado é o mesmo que foi revisado. Mesmo assim, cada
          addon é um programa: um addon instalado chega desligado, e o app pergunta antes de ligar.
        </p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1 1 auto;
    min-width: 0;
  }
  .content {
    max-width: 920px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .intro {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .segmented button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  /* Outlined in the text's color, which the chosen tab changes. */
  .count {
    min-width: 18px;
    padding: 0 5px;
    border: 1px solid currentColor;
    border-radius: 999px;
    font-size: 11px;
    line-height: 16px;
    text-align: center;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .with-icon {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .small-btn.with-icon {
    gap: 6px;
  }
  .path {
    margin-left: auto;
    max-width: 320px;
  }
  .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-text);
    font: inherit;
    text-decoration: underline;
  }
  .addon {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
    border: 1px solid var(--line-soft);
    border-radius: 16px;
    background: var(--raised);
  }
  .addon.invalid {
    background: transparent;
    border-style: dashed;
    border-color: var(--line-strong);
  }
  .addon.asking {
    border-color: var(--banner-line);
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }
  .tile {
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: var(--key);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .tile.bad {
    background: var(--error);
    color: var(--error-text);
  }
  .info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }
  h2 {
    font-size: 16px;
    font-weight: 600;
  }
  .badge {
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--accent);
    color: var(--accent-text);
    font-size: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 2px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--soft);
  }
  .warning {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--soft);
  }
  .warning :global(svg) {
    flex: none;
    color: var(--accent-text);
  }
  .side {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding-top: 2px;
  }
  .side.column {
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
  }
  .installed {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--soft);
  }
  .installed :global(svg) {
    color: var(--accent-text);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--muted);
  }
  .state.running {
    color: var(--soft);
  }
  .state.running .dot {
    border-color: var(--accent);
    background: var(--accent);
  }
  .state.stopped,
  .state.restarting {
    color: var(--error-text);
  }
  .state.stopped .dot,
  .state.restarting .dot {
    border-color: var(--error-text);
    background: var(--error-text);
  }
  .switch {
    position: relative;
    width: 44px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--line-strong);
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--muted);
    transition: left 0.15s;
  }
  .switch[aria-checked='true'] {
    background: var(--accent);
  }
  .switch[aria-checked='true'] span {
    left: 23px;
    background: var(--ink);
  }
  .problem {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--error);
    border: 1px solid var(--error-line);
    color: var(--error-text);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .info .buttons {
    margin-top: 4px;
  }
  .problem .secondary {
    color: var(--text);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border-radius: 12px;
    background: var(--banner);
    border: 1px solid var(--banner-line);
  }
  .confirm h3 {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 16px;
    font-weight: 600;
  }
  .confirm h3 :global(svg) {
    color: var(--accent-text);
  }
  dl {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
