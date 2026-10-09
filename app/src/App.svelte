<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import {
    getConfig,
    getStatus,
    getUpdate,
    glyphs as loadGlyphs,
    installUpdate,
    onStatus,
    onUpdate,
    onUpdateProgress,
    openConfig,
    resend,
    saveConfig,
    setPaused,
    simulate,
    type Update,
  } from './lib/api';
  import Icon from './lib/Icon.svelte';
  import LayerEditor from './lib/LayerEditor.svelte';
  import Live from './lib/Live.svelte';
  import NewRule from './lib/NewRule.svelte';
  import Settings from './lib/Settings.svelte';
  import { modeName, ruleTarget, type Config, type Glyph, type Key, type Rule, type Status } from './lib/types';

  type View = { kind: 'live' } | { kind: 'default' } | { kind: 'rule'; index: number } | { kind: 'new' } | { kind: 'settings' };

  const appWindow = getCurrentWindow();
  /** Edits closer together than this are one undo step (typing a word). */
  const UNDO_GROUP_MS = 800;
  const HISTORY_LIMIT = 100;

  let status = $state<Status | null>(null);
  let config = $state<Config | null>(null);
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let saveState = $state<'idle' | 'saving' | 'saved'>('idle');
  let glyphs = $state<Glyph[]>([]);
  let view = $state<View>({ kind: 'live' });
  let clipboard = $state<Key | null>(null);
  let maximized = $state(false);
  let canUndo = $state(false);
  let canRedo = $state(false);
  /** A newer version found on GitHub, and how far its download is. */
  let update = $state<Update | null>(null);
  let installing = $state(false);
  let downloaded = $state<number | null>(null);
  let updateError = $state<string | null>(null);

  // What the file holds, what the editors last showed, and the history between them.
  let lastSaved = '';
  let current = '';
  let undoStack: string[] = [];
  let redoStack: string[] = [];
  let lastChangeAt = 0;
  let fromHistory = false;
  let lastEdit = 0;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let savedTimer: ReturnType<typeof setTimeout> | undefined;
  let seenRevision = -1;

  let draggingRule = $state<number | null>(null);
  let overRule = $state<number | null>(null);

  async function loadConfig() {
    try {
      const loaded = await getConfig();
      const json = JSON.stringify(loaded);
      lastSaved = json;
      current = json;
      undoStack = [];
      redoStack = [];
      syncHistoryButtons();
      config = loaded;
      loadError = null;
    } catch (e) {
      loadError = String(e);
    }
  }

  onMount(() => {
    getStatus().then((s) => (status = s));
    const stopStatus = onStatus((s) => (status = s));
    loadGlyphs().then((g) => (glyphs = g));
    loadConfig();
    appWindow.isMaximized().then((m) => (maximized = m));
    const stopResize = appWindow.onResized(async () => (maximized = await appWindow.isMaximized()));
    getUpdate().then((u) => (update = u));
    const stopUpdate = onUpdate((u) => (update = u));
    const stopProgress = onUpdateProgress((percent) => (downloaded = percent));
    return () => {
      stopStatus.then((unlisten) => unlisten());
      stopResize.then((unlisten) => unlisten());
      stopUpdate.then((unlisten) => unlisten());
      stopProgress.then((unlisten) => unlisten());
    };
  });

  /** On Windows this never returns: the installer closes the app and opens the new version. */
  async function startUpdate() {
    installing = true;
    updateError = null;
    try {
      await installUpdate();
    } catch (e) {
      updateError = String(e);
      installing = false;
      downloaded = null;
    }
  }

  function syncHistoryButtons() {
    canUndo = undoStack.length > 0;
    canRedo = redoStack.length > 0;
  }

  // Every edit lands in the undo history and is saved shortly after.
  $effect(() => {
    if (!config) return;
    const json = JSON.stringify($state.snapshot(config));
    if (json === current) return;
    const now = Date.now();
    if (!fromHistory) {
      if (now - lastChangeAt > UNDO_GROUP_MS) {
        undoStack.push(current);
        if (undoStack.length > HISTORY_LIMIT) undoStack.shift();
      }
      redoStack = [];
      lastChangeAt = now;
    }
    fromHistory = false;
    current = json;
    syncHistoryButtons();
    if (json === lastSaved) return;
    lastEdit = now;
    saveState = 'saving';
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await saveConfig(JSON.parse(json));
        lastSaved = json;
        saveError = null;
        saveState = 'saved';
        clearTimeout(savedTimer);
        savedTimer = setTimeout(() => (saveState = 'idle'), 2000);
      } catch (e) {
        saveError = String(e);
        saveState = 'idle';
      }
    }, 350);
  });

  // Someone edited config.json by hand: reload it, unless it is our own save.
  $effect(() => {
    const revision = status?.config_revision;
    if (revision === undefined || revision === seenRevision) return;
    seenRevision = revision;
    if (Date.now() - lastEdit > 1500) loadConfig();
  });

  function travel(from: string[], to: string[]) {
    const target = from.pop();
    if (target === undefined) return;
    to.push(current);
    fromHistory = true;
    lastChangeAt = 0;
    config = JSON.parse(target);
    // The rule being edited may be gone in the other version.
    if (view.kind === 'rule' && config && !config.rules[view.index]) view = { kind: 'live' };
    syncHistoryButtons();
  }
  const undo = () => travel(undoStack, redoStack);
  const redo = () => travel(redoStack, undoStack);

  function onKey(event: KeyboardEvent) {
    if (!event.ctrlKey || event.altKey) return;
    // Text fields keep their own undo.
    if ((event.target as HTMLElement | null)?.closest('input, textarea, select, [contenteditable]')) return;
    const letter = event.key.toLowerCase();
    if (letter === 'z' && !event.shiftKey) {
      event.preventDefault();
      undo();
    } else if (letter === 'y' || (letter === 'z' && event.shiftKey)) {
      event.preventDefault();
      redo();
    }
  }

  function createRule(rule: Rule) {
    if (!config) return;
    config.rules.push(rule);
    view = { kind: 'rule', index: config.rules.length - 1 };
  }

  function moveRule(from: number, to: number) {
    if (!config || from === to) return;
    const editing = view.kind === 'rule' ? config.rules[view.index] : null;
    const [moved] = config.rules.splice(from, 1);
    config.rules.splice(to, 0, moved);
    if (editing) view = { kind: 'rule', index: config.rules.indexOf(editing) };
  }

  const isView = (kind: View['kind'], index?: number) =>
    view.kind === kind && (index === undefined || (view.kind === 'rule' && view.index === index));
</script>

<svelte:window onkeydown={onKey} />

{#if status}
  <div class="shell">
    <header class="titlebar" data-tauri-drag-region>
      <div class="brand" data-tauri-drag-region><Icon name="grid" size={20} /> D200 Deck</div>
      <div class="status" data-tauri-drag-region>
        <span class="pill" class:off={!status.device} data-tauri-drag-region>
          <span class="led"></span>{status.device ? 'D200 conectado' : 'D200 desconectado'}
        </span>
        <span class="pill" class:off={!status.extension} data-tauri-drag-region>
          <span class="led"></span>{status.extension ? 'Extensão conectada' : 'Extensão desconectada'}
        </span>
      </div>
      <div class="drag" data-tauri-drag-region></div>
      <div class="tools">
        <span class="save-state" aria-live="polite">
          {#if saveState === 'saving'}<span class="muted">Salvando…</span>
          {:else if saveState === 'saved'}<span class="saved"><Icon name="check" size={14} />Salvo</span>{/if}
        </span>
        <button type="button" class="tool icon" title="Desfazer (Ctrl+Z)" aria-label="Desfazer" disabled={!canUndo} onclick={undo}>
          <Icon name="undo" size={16} />
        </button>
        <button type="button" class="tool icon" title="Refazer (Ctrl+Y)" aria-label="Refazer" disabled={!canRedo} onclick={redo}>
          <Icon name="redo" size={16} />
        </button>
        <button type="button" class="tool" aria-pressed={status.paused} onclick={() => setPaused(!status!.paused)}>
          <Icon name={status.paused ? 'play' : 'pause'} size={16} />{status.paused ? 'Retomar regras' : 'Pausar regras'}
        </button>
        <button type="button" class="tool icon" title="Reenviar tudo para o D200" aria-label="Reenviar tudo para o D200" onclick={() => resend()}>
          <Icon name="refresh" size={16} />
        </button>
      </div>
      <div class="window-buttons">
        <button type="button" class="win" aria-label="Minimizar" title="Minimizar" onclick={() => appWindow.minimize()}>
          <Icon name="minimize" size={14} />
        </button>
        <button
          type="button"
          class="win"
          aria-label={maximized ? 'Restaurar' : 'Maximizar'}
          title={maximized ? 'Restaurar' : 'Maximizar'}
          onclick={() => appWindow.toggleMaximize()}
        >
          <Icon name={maximized ? 'restore' : 'maximize'} size={13} />
        </button>
        <button type="button" class="win close" aria-label="Fechar" title="Fechar (o D200 Deck continua na bandeja)" onclick={() => appWindow.close()}>
          <Icon name="close" size={14} />
        </button>
      </div>
    </header>

    {#if status.config_error}
      <div class="banner error" role="alert">
        <Icon name="alert" />
        <span><strong>O config.json tem um erro e não foi aplicado.</strong> {status.config_error}</span>
        <button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button>
      </div>
    {/if}
    {#if saveError}
      <div class="banner error" role="alert">
        <Icon name="alert" />
        <span><strong>Mudança ainda não salva:</strong> {saveError}</span>
      </div>
    {/if}
    {#if update}
      <div class="banner" role="status">
        <Icon name="refresh" />
        {#if installing}
          <span>Baixando a versão {update.version}{downloaded === null ? '' : ` (${downloaded}%)`}. O D200 Deck fecha e abre de novo sozinho.</span>
        {:else}
          <span>
            <strong>Versão {update.version} disponível.</strong>
            {updateError ? `A atualização falhou: ${updateError}` : 'O app fecha por alguns segundos e volta na versão nova.'}
          </span>
          <button type="button" class="primary" onclick={startUpdate}>{updateError ? 'Tentar de novo' : 'Atualizar e reiniciar'}</button>
        {/if}
      </div>
    {/if}
    {#if status.simulation}
      <div class="banner">
        <Icon name="live" />
        <span>Simulando contexto: o D200 mostra as regras do simulador, não o que está aberto.</span>
        <button type="button" class="secondary" onclick={() => simulate(null)}>Voltar ao real</button>
      </div>
    {/if}

    <div class="body">
      <nav class="scroll" aria-label="Seções">
        <button type="button" class="nav-item" aria-current={isView('live') ? 'page' : undefined} onclick={() => (view = { kind: 'live' })}>
          <Icon name="live" />
          <span class="nav-text"><span>Ao vivo</span><span class="muted small">o que o D200 mostra agora</span></span>
        </button>
        <button type="button" class="nav-item" aria-current={isView('default') ? 'page' : undefined} onclick={() => (view = { kind: 'default' })}>
          <Icon name="grid" />
          <span class="nav-text"><span>Layout padrão</span><span class="muted small">quando nenhuma regra vale</span></span>
        </button>

        <div class="nav-title">
          <span>Regras</span>
          <button type="button" class="new" aria-current={isView('new') ? 'page' : undefined} onclick={() => (view = { kind: 'new' })}>
            <Icon name="plus" size={14} />Nova
          </button>
        </div>
        {#if config}
          {#if config.rules.length === 0}
            <p class="muted small nav-note">Nenhuma regra ainda. Crie a primeira em "Nova".</p>
          {:else}
            <p class="muted small nav-note">Arraste para mudar a ordem.</p>
          {/if}
          {#each config.rules as rule, i (i)}
            {@const target = ruleTarget(rule)}
            <button
              type="button"
              class="nav-item rule"
              class:disabled={!rule.enabled}
              class:dragging={draggingRule === i}
              class:over-before={overRule === i && draggingRule !== null && draggingRule > i}
              class:over-after={overRule === i && draggingRule !== null && draggingRule < i}
              draggable="true"
              aria-current={isView('rule', i) ? 'page' : undefined}
              onclick={() => (view = { kind: 'rule', index: i })}
              ondragstart={(e) => {
                draggingRule = i;
                e.dataTransfer?.setData('text/plain', String(i));
                if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
              }}
              ondragend={() => {
                draggingRule = null;
                overRule = null;
              }}
              ondragover={(e) => {
                if (draggingRule === null) return;
                e.preventDefault();
                overRule = i;
              }}
              ondrop={(e) => {
                e.preventDefault();
                if (draggingRule !== null) moveRule(draggingRule, i);
                draggingRule = null;
                overRule = null;
              }}
            >
              <Icon name={target.kind} />
              <span class="nav-text">
                <span class="rule-name">
                  <span class="ellipsis">{rule.name || 'Sem nome'}</span>
                  {#if status.rules[i]?.active}<span class="valid"><span class="led"></span>valendo</span>{/if}
                </span>
                <span class="mono muted ellipsis">{target.target}</span>
              </span>
              <span class="badge" class:focus={rule.mode === 'focus'}>{modeName(rule.mode)}</span>
            </button>
          {/each}
        {/if}

        <div class="spacer"></div>
        <button type="button" class="nav-item" aria-current={isView('settings') ? 'page' : undefined} onclick={() => (view = { kind: 'settings' })}>
          <Icon name="settings" /><span>Ajustes</span>
        </button>
      </nav>

      {#if view.kind === 'live'}
        <Live {status} />
      {:else if !config}
        <div class="page">
          <h1>Não deu para abrir a configuração</h1>
          <p class="muted">{loadError ?? 'Carregando…'}</p>
          <div><button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button></div>
        </div>
      {:else if view.kind === 'default'}
        <LayerEditor
          bind:config
          index={null}
          active={false}
          {glyphs}
          edgePages={status.edge_pages}
          {clipboard}
          oncopy={(key) => (clipboard = key)}
          onmoved={() => {}}
          ondeleted={() => {}}
        />
      {:else if view.kind === 'rule' && config.rules[view.index]}
        {#key view.index}
          <LayerEditor
            bind:config
            index={view.index}
            active={status.rules[view.index]?.active ?? false}
            {glyphs}
            edgePages={status.edge_pages}
            {clipboard}
            oncopy={(key) => (clipboard = key)}
            onmoved={(to) => (view = { kind: 'rule', index: to })}
            ondeleted={() => (view = { kind: 'live' })}
          />
        {/key}
      {:else if view.kind === 'new'}
        <NewRule edgeTabs={status.edge_tabs} extension={status.extension} oncreate={createRule} oncancel={() => (view = { kind: 'live' })} />
      {:else if view.kind === 'settings'}
        <Settings bind:config configPath={status.config_path} />
      {/if}
    </div>
  </div>
{/if}

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .titlebar {
    flex: none;
    height: 44px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding-left: 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line-soft);
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 600;
    font-size: 14px;
    white-space: nowrap;
  }
  .brand :global(.ico) {
    color: var(--accent);
    pointer-events: none;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-left: 8px;
  }
  .drag {
    flex: 1 1 auto;
    align-self: stretch;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--soft);
    white-space: nowrap;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    pointer-events: none;
  }
  .pill.off .led {
    background: transparent;
    border: 1.5px solid var(--muted);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .save-state {
    min-width: 62px;
    font-size: 12px;
    text-align: right;
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent-text);
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: transparent;
    font-size: 13px;
    white-space: nowrap;
  }
  .tool.icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .tool:hover:not(:disabled) {
    background: var(--hover);
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .tool[aria-pressed='true'] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ink);
  }
  .window-buttons {
    display: flex;
    align-self: stretch;
    margin-left: 6px;
  }
  .win {
    width: 46px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--soft);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .win:hover {
    background: var(--hover);
    color: var(--text);
  }
  .win.close:hover {
    background: var(--close-hover);
    color: #fff;
  }
  .banner {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px;
    background: var(--banner);
    border-bottom: 1px solid var(--banner-line);
  }
  .banner.error {
    background: var(--error);
    border-bottom-color: var(--error-line);
  }
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }
  nav {
    flex: none;
    width: 264px;
    padding: 14px 12px;
    background: var(--panel);
    border-right: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--soft);
    text-align: left;
  }
  .nav-item:hover {
    background: var(--hover);
  }
  .nav-item[aria-current='page'] {
    background: var(--selected);
    color: var(--text);
  }
  .nav-item.disabled {
    opacity: 0.55;
  }
  .nav-item.dragging {
    opacity: 0.4;
  }
  .nav-item.over-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .nav-item.over-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }
  .nav-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1 1 auto;
  }
  .nav-title {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 10px 4px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .new {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 8px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: transparent;
    font-size: 13px;
    font-weight: 500;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text);
  }
  .new:hover {
    background: var(--hover);
  }
  .new[aria-current='page'] {
    border-color: var(--accent);
  }
  .nav-note {
    padding: 0 10px 6px;
  }
  .rule-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
    color: var(--text);
    min-width: 0;
  }
  .valid {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 500;
    color: var(--soft);
    flex: none;
  }
  .valid .led {
    width: 6px;
    height: 6px;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .spacer {
    flex: 1 1 auto;
    min-height: 12px;
  }
  .page {
    flex: 1 1 auto;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
