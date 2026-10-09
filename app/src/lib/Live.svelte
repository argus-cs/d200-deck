<script lang="ts">
  import { onMount } from 'svelte';
  import { onActivity, simulate } from './api';
  import Deck from './Deck.svelte';
  import ExtensionGuide from './ExtensionGuide.svelte';
  import Simulator from './Simulator.svelte';
  import { modeName, position, screenLabel, type Activity, type Status } from './types';

  let { status }: { status: Status } = $props();

  const GUIDE_DISMISSED = 'd200deck.extensionGuideDismissed';
  const LIGHT_MS = 450;

  let selected = $state(1);
  let lit = $state<number[]>([]);
  let feed = $state<(Activity & { id: number; time: string })[]>([]);
  let guideDismissed = $state(readDismissed());
  let nextId = 0;

  function readDismissed() {
    try {
      return localStorage.getItem(GUIDE_DISMISSED) === '1';
    } catch {
      return false;
    }
  }

  function dismissGuide() {
    guideDismissed = true;
    try {
      localStorage.setItem(GUIDE_DISMISSED, '1');
    } catch {
      // Only remembered for this session then.
    }
  }

  onMount(() => {
    const stop = onActivity((activity) => {
      const time = new Date().toLocaleTimeString('pt-BR');
      feed = [{ ...activity, id: nextId++, time }, ...feed].slice(0, 8);
      if (activity.error) return;
      // Restart the animation when the same key is pressed again quickly.
      lit = lit.filter((n) => n !== activity.number);
      requestAnimationFrame(() => {
        lit = [...lit, activity.number];
        setTimeout(() => (lit = lit.filter((n) => n !== activity.number)), LIGHT_MS);
      });
    });
    return () => {
      stop.then((unlisten) => unlisten());
    };
  });

  const key = $derived(selected === 14 ? null : (status.keys.find((k) => k.number === selected) ?? null));
  const active = $derived(status.rules.filter((r) => r.active));
  const disputes = $derived(status.keys.filter((k) => k.beaten.length > 0));
  const needsExtension = $derived(!status.extension && status.rules.some((r) => r.kind === 'site'));
  const focusedText = $derived.by(() => {
    if (!status.focused) return 'nada';
    return status.focused_site ? `${status.focused} · ${status.focused_site}` : status.focused;
  });
</script>

<div class="layout">
  <main class="scroll">
    <div>
      <h1>Ao vivo</h1>
      <p class="muted">O que o D200 está mostrando agora. O ponto marca as teclas trocadas por uma regra, e cada tecla apertada acende aqui.</p>
    </div>
    {#if needsExtension && !guideDismissed}
      <ExtensionGuide ondismiss={dismissGuide} />
    {/if}
    <div class="chips">
      <span class="chip"><span class="muted">Em foco</span><span class="mono">{focusedText}</span></span>
      <span class="chip"><span class="muted">Abertos</span><span class="mono">{status.open.join(', ') || 'nenhum com regra'}</span></span>
      {#if status.folder}
        <span class="chip folder"><span class="muted">Pasta aberta</span><span>{status.folder}</span></span>
      {/if}
    </div>

    <Deck keys={status.keys} {selected} {lit} screen={status.screen} onselect={(n) => (selected = n)} />

    <div class="cards">
      <Simulator rules={status.rules} simulation={status.simulation} onchange={(s) => simulate(s)} />
      <section class="card">
        <h2>Regras valendo agora</h2>
        {#if status.paused}
          <p class="muted">Regras pausadas: o D200 está no layout padrão.</p>
        {:else if active.length === 0}
          <p class="muted">Nenhuma. O D200 está no layout padrão.</p>
        {/if}
        {#each active as rule, i (i)}
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

  <aside class="scroll" aria-label="Tecla selecionada e últimas teclas">
    {#if selected === 14}
      <div>
        <div class="muted small">Tecla 14</div>
        <h2>Visor</h2>
      </div>
      <div class="facts">
        <div><div class="muted small">Mostrando</div><div>{screenLabel(status.screen.content)}</div></div>
        <div><div class="muted small">Vem de</div><div>{status.screen.rule ?? 'Layout padrão'}</div></div>
      </div>
    {:else if key}
      <div>
        <div class="muted small">{position(key.number)}</div>
        <h2>Tecla {key.number}</h2>
      </div>
      {#if key.image}<img class="preview" src={key.image} alt={`Imagem da tecla ${key.number}`} />{/if}
      <div class="facts">
        <div><div class="muted small">Texto</div><div>{key.label || '—'}</div></div>
        <div>
          <div class="muted small">Vem de</div>
          <div>{key.rule && key.mode ? `${key.rule} · ${modeName(key.mode)}` : 'Layout padrão'}</div>
        </div>
        <div><div class="muted small">Ao apertar</div><div class="mono">{key.action ?? 'Nenhuma ação'}</div></div>
        {#if key.toggled}
          <div><div class="muted small">Estado</div><div>Mostrando o segundo estado</div></div>
        {/if}
        {#if key.beaten.length > 0}
          <div><div class="muted small">Também queriam esta tecla</div><div>{key.beaten.join(', ')}</div></div>
        {/if}
      </div>
    {/if}

    <div class="feed">
      <h3>Últimas teclas</h3>
      {#if feed.length === 0}
        <p class="muted small">Aperte uma tecla no D200 para ver o que ela faz aqui.</p>
      {/if}
      <ol aria-live="polite">
        {#each feed as item (item.id)}
          <li class:failed={!!item.error}>
            <div class="feed-top">
              <strong>Tecla {item.number}</strong>
              <span class="muted small">{item.label || 'sem texto'}{item.rule ? ` · ${item.rule}` : ''}</span>
              <span class="muted small time">{item.time}</span>
            </div>
            {#if item.error}
              <div class="error-text small">Falhou: {item.error}</div>
            {:else}
              <div class="mono small">{item.action ?? 'sem ação'}</div>
            {/if}
          </li>
        {/each}
      </ol>
    </div>
  </aside>
</div>

<style>
  .layout {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
  }
  main {
    flex: 1 1 auto;
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
    border: 1px solid var(--line-soft);
    font-size: 13px;
  }
  .chip.folder {
    border-color: var(--accent);
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
  h3 {
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
    flex: none;
    width: 320px;
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
    border: 1px solid var(--line-soft);
    border-radius: 12px;
  }
  .feed {
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-top: 1px solid var(--line-soft);
    padding-top: 16px;
  }
  .feed ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .feed li {
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .feed li.failed {
    border-color: var(--error-line);
    background: var(--error);
  }
  .feed-top {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }
  .feed-top .muted:not(.time) {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .time {
    flex: none;
  }
</style>
