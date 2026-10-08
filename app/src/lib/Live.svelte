<script lang="ts">
  import { simulate } from './api';
  import Deck from './Deck.svelte';
  import Simulator from './Simulator.svelte';
  import { modeName, position, type Status } from './types';

  let { status }: { status: Status } = $props();

  let selected = $state(1);

  const key = $derived(status.keys.find((k) => k.number === selected) ?? null);
  const active = $derived(status.rules.filter((r) => r.active));
  const disputes = $derived(status.keys.filter((k) => k.beaten.length > 0));
  const focusedText = $derived.by(() => {
    if (!status.focused) return 'nada';
    return status.focused_site ? `${status.focused} · ${status.focused_site}` : status.focused;
  });
</script>

<div class="layout">
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
          <div>{key.rule && key.mode ? `${key.rule} · ${modeName(key.mode)}` : 'Layout padrão'}</div>
        </div>
        <div><div class="muted small">Ao apertar</div><div class="mono">{key.action ?? 'Nenhuma ação'}</div></div>
        {#if key.beaten.length > 0}
          <div><div class="muted small">Também queriam esta tecla</div><div>{key.beaten.join(', ')}</div></div>
        {/if}
      </div>
    {/if}
  </aside>
</div>

<style>
  .layout {
    flex: 999 1 640px;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
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
