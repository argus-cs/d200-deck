<script lang="ts">
  import { onMount } from 'svelte';
  import { getConfig, getStatus, glyphs as loadGlyphs, onStatus, openConfig, resend, saveConfig, setPaused, simulate } from './lib/api';
  import Icon from './lib/Icon.svelte';
  import LayerEditor from './lib/LayerEditor.svelte';
  import Live from './lib/Live.svelte';
  import NewRule from './lib/NewRule.svelte';
  import Settings from './lib/Settings.svelte';
  import { modeName, ruleTarget, type Config, type Rule, type Status } from './lib/types';

  type View = { kind: 'live' } | { kind: 'default' } | { kind: 'rule'; index: number } | { kind: 'new' } | { kind: 'settings' };

  let status = $state<Status | null>(null);
  let config = $state<Config | null>(null);
  let loadError = $state<string | null>(null);
  let saveError = $state<string | null>(null);
  let glyphs = $state<string[]>([]);
  let view = $state<View>({ kind: 'live' });

  // Autosave bookkeeping: what the file holds, and when the person last edited.
  let lastSaved = '';
  let lastEdit = 0;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let seenRevision = -1;

  async function loadConfig() {
    try {
      const loaded = await getConfig();
      lastSaved = JSON.stringify(loaded);
      config = loaded;
      loadError = null;
    } catch (e) {
      loadError = String(e);
    }
  }

  onMount(() => {
    getStatus().then((s) => (status = s));
    const stop = onStatus((s) => (status = s));
    loadGlyphs().then((g) => (glyphs = g));
    loadConfig();
    return () => {
      stop.then((unlisten) => unlisten());
    };
  });

  // Every edit is validated and written shortly after the last keystroke.
  $effect(() => {
    if (!config) return;
    const json = JSON.stringify($state.snapshot(config));
    if (json === lastSaved) return;
    lastEdit = Date.now();
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      try {
        await saveConfig(JSON.parse(json));
        lastSaved = json;
        saveError = null;
      } catch (e) {
        saveError = String(e);
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

  function createRule(rule: Rule) {
    if (!config) return;
    config.rules.push(rule);
    view = { kind: 'rule', index: config.rules.length - 1 };
  }

  const isView = (kind: View['kind'], index?: number) =>
    view.kind === kind && (index === undefined || (view.kind === 'rule' && view.index === index));
</script>

{#if status}
  <div class="shell">
    <header class="top">
      <div class="brand"><Icon name="grid" size={22} /> D200 Deck</div>
      <div class="spacer"></div>
      <span class="pill" class:off={!status.device}><span class="led"></span>{status.device ? 'D200 conectado' : 'D200 desconectado'}</span>
      <span class="pill" class:off={!status.extension}>
        <span class="led"></span>{status.extension ? 'Extensão do Edge conectada' : 'Extensão do Edge desconectada'}
      </span>
      <button type="button" class="tool" aria-pressed={status.paused} onclick={() => setPaused(!status!.paused)}>
        <Icon name={status.paused ? 'play' : 'pause'} size={16} />{status.paused ? 'Retomar regras' : 'Pausar regras'}
      </button>
      <button type="button" class="tool" onclick={() => resend()}><Icon name="refresh" size={16} />Reenviar</button>
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
    {#if status.simulation}
      <div class="banner">
        <Icon name="live" />
        <span>Simulando contexto: o D200 mostra as regras do simulador, não o que está aberto.</span>
        <button type="button" class="secondary" onclick={() => simulate(null)}>Voltar ao real</button>
      </div>
    {/if}

    <div class="body">
      <nav aria-label="Seções">
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
          {/if}
          {#each config.rules as rule, i (i)}
            {@const target = ruleTarget(rule)}
            <button
              type="button"
              class="nav-item rule"
              class:disabled={!rule.enabled}
              aria-current={isView('rule', i) ? 'page' : undefined}
              onclick={() => (view = { kind: 'rule', index: i })}
            >
              <Icon name={target.kind} />
              <span class="nav-text">
                <span class="rule-name">
                  {rule.name || 'Sem nome'}
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
          <Icon name="file" /><span>Ajustes</span>
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
        <LayerEditor bind:config index={null} active={false} {glyphs} onmoved={() => {}} ondeleted={() => {}} />
      {:else if view.kind === 'rule' && config.rules[view.index]}
        {#key view.index}
          <LayerEditor
            bind:config
            index={view.index}
            active={status.rules[view.index]?.active ?? false}
            {glyphs}
            onmoved={(to) => (view = { kind: 'rule', index: to })}
            ondeleted={() => (view = { kind: 'live' })}
          />
        {/key}
      {:else if view.kind === 'new'}
        <NewRule edgeTabs={status.edge_tabs} oncreate={createRule} oncancel={() => (view = { kind: 'live' })} />
      {:else if view.kind === 'settings'}
        <Settings bind:config configPath={status.config_path} />
      {/if}
    </div>
  </div>
{/if}

<style>
  .shell {
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 14px;
    padding: 12px 20px;
    background: var(--panel);
    border-bottom: 1px solid var(--line-soft);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 600;
    font-size: 15px;
  }
  .brand :global(.ico) {
    color: var(--accent);
  }
  .spacer {
    flex: 1 1 auto;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    color: var(--soft);
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }
  .pill.off .led {
    background: transparent;
    border: 1.5px solid var(--muted);
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: transparent;
    font-size: 13px;
  }
  .tool[aria-pressed='true'] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ink);
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px;
    background: #2a2111;
    border-bottom: 1px solid #4a3a1c;
  }
  .banner.error {
    background: #2e1515;
    border-bottom-color: #5a2424;
  }
  .body {
    flex: 1 1 auto;
    display: flex;
    flex-wrap: wrap;
  }
  nav {
    flex: 1 1 240px;
    max-width: 280px;
    padding: 16px 12px;
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
    background: #1b1c20;
  }
  .nav-item[aria-current='page'] {
    background: var(--key);
    color: var(--text);
  }
  .nav-item.disabled {
    opacity: 0.55;
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
    padding: 18px 10px 6px;
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
  .new[aria-current='page'] {
    border-color: var(--accent);
  }
  .nav-note {
    padding: 0 10px;
  }
  .rule-name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 500;
    color: var(--text);
  }
  .valid {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    font-weight: 500;
    color: var(--soft);
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
  .page {
    flex: 999 1 640px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
