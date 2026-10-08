<script lang="ts">
  import { onMount } from 'svelte';
  import { getStatus, onStatus, openConfig, resend, setPaused, simulate } from './lib/api';
  import Deck from './lib/Deck.svelte';
  import Icon from './lib/Icon.svelte';
  import Simulator from './lib/Simulator.svelte';
  import { modeName, type Status } from './lib/types';

  let status = $state<Status | null>(null);
  let selected = $state(1);

  onMount(() => {
    getStatus().then((s) => (status = s));
    const stop = onStatus((s) => (status = s));
    return () => {
      stop.then((unlisten) => unlisten());
    };
  });

  const key = $derived(status?.keys.find((k) => k.number === selected) ?? null);
  const active = $derived(status?.rules.filter((r) => r.active) ?? []);
  const disputes = $derived(status?.keys.filter((k) => k.beaten.length > 0) ?? []);
  const focusedText = $derived.by(() => {
    if (!status?.focused) return 'nada';
    return status.focused_site ? `${status.focused} · ${status.focused_site}` : status.focused;
  });
  const position = (n: number) => (n === 14 ? 'Visor' : `Linha ${Math.ceil(n / 5)} · coluna ${((n - 1) % 5) + 1}`);
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
      <button type="button" class="tool" onclick={() => openConfig()}><Icon name="file" size={16} />Editar config</button>
    </header>

    {#if status.config_error}
      <div class="banner error" role="alert">
        <Icon name="alert" />
        <span><strong>A config tem um erro e não foi aplicada.</strong> {status.config_error}</span>
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
        <span class="nav-item current" aria-current="page"><Icon name="live" />Ao vivo</span>
        <div class="nav-title">Regras</div>
        {#if status.rules.length === 0}
          <p class="muted small nav-note">Nenhuma regra ainda. Use "Editar config" para criar.</p>
        {/if}
        <ul>
          {#each status.rules as rule, i (i)}
            <li class:disabled={!rule.enabled}>
              <Icon name={rule.kind} />
              <span class="rule-text">
                <span class="rule-name">
                  {rule.name}
                  {#if rule.active}<span class="valid"><span class="led"></span>valendo</span>{/if}
                </span>
                <span class="mono muted ellipsis">{rule.target}</span>
              </span>
              <span class="badge" class:focus={rule.mode === 'focus'}>{modeName(rule.mode)}</span>
            </li>
          {/each}
        </ul>
        <div class="spacer"></div>
        <p class="muted small nav-note mono ellipsis" title={status.config_path}>{status.config_path}</p>
      </nav>

      <main>
        <div>
          <h1>Ao vivo</h1>
          <p class="muted">O que o D200 está mostrando agora. O ponto marca as teclas trocadas por uma regra.</p>
        </div>
        <div class="chips">
          <span class="chip"><span class="muted">Em foco</span><span class="mono">{focusedText}</span></span>
          <span class="chip"><span class="muted">Abertos</span><span class="mono">{status.open.join(', ') || 'nenhum com regra'}</span></span>
        </div>

        <Deck keys={status.keys} {selected} window={status.window} onselect={(n) => (selected = n)} />

        <div class="cards">
          <Simulator rules={status.rules} simulation={status.simulation} onchange={(s) => simulate(s)} />
          <section class="card">
            <h2>Regras valendo agora</h2>
            {#if status.paused}
              <p class="muted">Regras pausadas: o D200 está no layout padrão.</p>
            {:else if active.length === 0}
              <p class="muted">Nenhuma. O D200 está no layout padrão.</p>
            {/if}
            {#each active as rule (rule.name + rule.mode)}
              <div class="active-row">
                <span class="grow">{rule.name}</span>
                <span class="badge" class:focus={rule.mode === 'focus'}>{modeName(rule.mode)}</span>
              </div>
            {/each}
            {#if disputes.length > 0}
              <div class="disputes">
                <h3>Disputas</h3>
                {#each disputes as k (k.number)}
                  <p class="small">Tecla {k.number}: {k.rule} venceu {k.beaten.join(', ')}.</p>
                {/each}
              </div>
            {/if}
            <p class="muted small push">Foco ganha de aberto. Entre regras do mesmo modo, vence a que estiver mais abaixo na lista.</p>
          </section>
        </div>
      </main>

      <aside aria-label="Tecla selecionada">
        {#if key}
          <div>
            <div class="muted small">{position(key.number)}</div>
            <h2>Tecla {key.number}</h2>
          </div>
          {#if key.image}<img class="preview" src={key.image} alt={`Imagem da tecla ${key.number}`} />{/if}
          <div class="facts">
            <div><div class="muted small">Texto</div><div>{key.label || '—'}</div></div>
            <div>
              <div class="muted small">Vem de</div>
              <div>{key.rule ? `${key.rule} · ${modeName(key.mode!)}` : 'Layout padrão'}</div>
            </div>
            <div><div class="muted small">Ao apertar</div><div class="mono">{key.action ?? 'Nenhuma ação'}</div></div>
            {#if key.beaten.length > 0}
              <div><div class="muted small">Também queriam esta tecla</div><div>{key.beaten.join(', ')}</div></div>
            {/if}
          </div>
        {/if}
      </aside>
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
    color: var(--text);
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
    gap: 4px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 10px;
    font-weight: 500;
  }
  .nav-item.current {
    background: var(--key);
  }
  .nav-title {
    padding: 18px 10px 6px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .nav-note {
    padding: 0 10px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 10px;
    color: var(--soft);
  }
  li.disabled {
    opacity: 0.5;
  }
  .rule-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1 1 auto;
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
  main {
    flex: 999 1 520px;
    min-width: 0;
    padding: 24px 28px 32px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h1 {
    font-size: 22px;
    font-weight: 600;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    display: flex;
    gap: 6px;
    align-items: center;
    padding: 5px 12px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid #2e3036;
    font-size: 13px;
  }
  .cards {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
  }
  .active-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .grow {
    flex: 1 1 auto;
    font-weight: 500;
  }
  .disputes {
    border-top: 1px solid var(--line-soft);
    padding-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .disputes h3 {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .push {
    margin-top: auto;
  }
  aside {
    flex: 1 1 280px;
    max-width: 340px;
    padding: 24px 20px;
    background: var(--panel);
    border-left: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  aside h2 {
    font-size: 18px;
    font-weight: 600;
  }
  .preview {
    width: 120px;
    height: 120px;
    border-radius: 14px;
    border: 1px solid var(--line);
  }
  .facts {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    background: var(--surface);
    border: 1px solid #2e3036;
    border-radius: 12px;
  }
</style>
