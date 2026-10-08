<script lang="ts">
  import EditDeck from './EditDeck.svelte';
  import KeyEditor from './KeyEditor.svelte';
  import { ruleTarget, type Config, type Mode } from './types';

  let {
    config = $bindable(),
    index,
    active,
    glyphs,
    onmoved,
    ondeleted,
  }: {
    config: Config;
    /** The rule being edited; null edits the default layout. */
    index: number | null;
    active: boolean;
    glyphs: string[];
    onmoved: (to: number) => void;
    ondeleted: () => void;
  } = $props();

  let selected = $state(1);
  let confirmDelete = $state(false);

  const rule = $derived(index === null ? null : config.rules[index]);
  const target = $derived(rule ? ruleTarget(rule) : null);
  const explain = $derived.by(() => {
    if (!rule || !target) return '';
    const t = target.target || '…';
    if (target.kind === 'app') {
      return rule.mode === 'open'
        ? `Vale enquanto ${t} estiver rodando, mesmo minimizado.`
        : `Vale só com a janela de ${t} na frente.`;
    }
    return rule.mode === 'open'
      ? `Vale enquanto houver uma aba de ${t} aberta no Edge, mesmo em segundo plano.`
      : `Vale só com o Edge na frente e uma aba de ${t} selecionada.`;
  });
  const overrides = $derived(rule ? Object.keys(rule.keys).length : 0);

  function setTarget(value: string) {
    if (!rule || !target) return;
    rule.when = target.kind === 'app' ? { process: value } : { site: value };
  }

  function setMode(mode: Mode) {
    if (rule) rule.mode = mode;
  }

  function move(delta: number) {
    if (index === null) return;
    const to = index + delta;
    if (to < 0 || to >= config.rules.length) return;
    const [moved] = config.rules.splice(index, 1);
    config.rules.splice(to, 0, moved);
    onmoved(to);
  }

  function remove() {
    if (index === null) return;
    config.rules.splice(index, 1);
    ondeleted();
  }
</script>

<div class="layout">
  <main>
    {#if rule && target && index !== null}
      <div class="rule-head">
        <div class="title-row">
          <input class="title" aria-label="Nome da regra" bind:value={rule.name} />
          {#if active}<span class="valid"><span class="led"></span>valendo agora</span>{/if}
        </div>
        <div class="when">
          <label for="rule-target" class="muted">{target.kind === 'app' ? 'Quando o app' : 'Quando o site'}</label>
          <input
            id="rule-target"
            class="mono"
            value={target.target}
            oninput={(e) => setTarget(e.currentTarget.value)}
            placeholder={target.kind === 'app' ? 'Discord.exe' : 'youtube.com'}
          />
          <div class="segmented" role="group" aria-label="Modo">
            <button type="button" aria-pressed={rule.mode === 'open'} onclick={() => setMode('open')}>Aberto</button>
            <button type="button" aria-pressed={rule.mode === 'focus'} onclick={() => setMode('focus')}>Foco</button>
          </div>
        </div>
        <p class="soft">{explain} <span class="muted">Troca {overrides} {overrides === 1 ? 'tecla' : 'teclas'} de 14.</span></p>
        <div class="tools">
          <label class="toggle"><input type="checkbox" bind:checked={rule.enabled} /> Regra ligada</label>
          <span class="spacer"></span>
          <button type="button" class="secondary" disabled={index === 0} onclick={() => move(-1)}>Subir</button>
          <button type="button" class="secondary" disabled={index === config.rules.length - 1} onclick={() => move(1)}>Descer</button>
          {#if confirmDelete}
            <button type="button" class="danger" onclick={remove}>Excluir de vez</button>
            <button type="button" class="secondary" onclick={() => (confirmDelete = false)}>Cancelar</button>
          {:else}
            <button type="button" class="secondary" onclick={() => (confirmDelete = true)}>Excluir regra</button>
          {/if}
        </div>
        <p class="muted small">A ordem decide as disputas entre regras do mesmo modo: a de baixo vence.</p>
      </div>
      <EditDeck keys={rule.keys} inherited={config.keys} {selected} onselect={(n) => (selected = n)} />
      <div class="legend">
        <span><span class="swatch override"></span>Trocada nesta regra</span>
        <span><span class="swatch inherited"></span>Vem do layout padrão (clique para sobrescrever)</span>
      </div>
    {:else}
      <div>
        <h1>Layout padrão</h1>
        <p class="muted">É o que o D200 mostra quando nenhuma regra está valendo. Cada regra troca só as teclas que sobrescreve.</p>
      </div>
      <EditDeck keys={config.keys} {selected} onselect={(n) => (selected = n)} />
    {/if}
    <p class="muted small">As mudanças são salvas sozinhas e aparecem no D200 na hora.</p>
  </main>

  <aside aria-label="Tecla selecionada">
    {#if rule && index !== null}
      <KeyEditor bind:keys={config.rules[index].keys} number={selected} inherited={config.keys[selected] ?? null} inRule {glyphs} />
    {:else}
      <KeyEditor bind:keys={config.keys} number={selected} inRule={false} {glyphs} />
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
    gap: 16px;
  }
  aside {
    flex: 1 1 300px;
    max-width: 360px;
    padding: 24px 20px;
    background: var(--panel);
    border-left: 1px solid var(--line-soft);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  h1 {
    font-size: 22px;
    font-weight: 600;
  }
  .rule-head {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .title {
    font: inherit;
    font-size: 22px;
    font-weight: 600;
    color: var(--text);
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    padding: 2px 6px;
    margin-left: -7px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .title:hover,
  .title:focus {
    border-color: var(--line);
  }
  .valid {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--soft);
    white-space: nowrap;
  }
  .led {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }
  .when {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }
  .when input {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 7px 10px;
    color: var(--text);
    min-width: 220px;
  }
  .segmented {
    display: flex;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 11px;
  }
  .segmented button {
    border: 0;
    border-radius: 8px;
    padding: 6px 14px;
    background: transparent;
    color: var(--soft);
    font-weight: 500;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--ink);
  }
  .soft {
    color: var(--soft);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .spacer {
    flex: 1 1 auto;
  }
  .toggle {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .toggle input {
    accent-color: var(--accent);
  }
  .danger {
    background: #8f2a2a;
    border: 0;
    border-radius: 10px;
    padding: 9px 14px;
    font-weight: 600;
    color: #fff;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 8px 20px;
    font-size: 13px;
    color: var(--muted);
  }
  .legend > span {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
  }
  .swatch.override {
    border: 2px solid var(--accent);
    background: var(--key);
  }
  .swatch.inherited {
    border: 1px dashed #4a4e57;
    background: var(--panel);
  }
</style>
