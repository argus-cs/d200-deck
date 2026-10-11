<script lang="ts">
  import Icon from './Icon.svelte';
  import { modeName, ruleActive, ruleTarget, type Rule, type RuleStatus, type View } from './types';

  let {
    view = $bindable(),
    rules,
    statuses,
    onmove,
  }: {
    view: View;
    /** `null` while the config isn't loaded. */
    rules: Rule[] | null;
    /** Which rules apply now, as the engine has them. */
    statuses: RuleStatus[];
    onmove: (from: number, to: number) => void;
  } = $props();

  let draggingRule = $state<number | null>(null);
  let overRule = $state<number | null>(null);

  const isView = (kind: View['kind'], index?: number) =>
    view.kind === kind && (index === undefined || (view.kind === 'rule' && view.index === index));
</script>

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
  {#if rules}
    {#if rules.length === 0}
      <p class="muted small nav-note">Nenhuma regra ainda. Crie a primeira em "Nova".</p>
    {:else}
      <p class="muted small nav-note">Arraste para mudar a ordem.</p>
    {/if}
    {#each rules as rule, i (i)}
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
          if (draggingRule !== null) onmove(draggingRule, i);
          draggingRule = null;
          overRule = null;
        }}
      >
        <Icon name={target.kind} />
        <span class="nav-text">
          <span class="rule-name">
            <span class="ellipsis">{rule.name || 'Sem nome'}</span>
            {#if ruleActive(statuses, rule)}<span class="valid"><span class="led"></span>valendo</span>{/if}
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

<style>
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
    border-radius: 50%;
    background: var(--accent);
    pointer-events: none;
  }
  .spacer {
    flex: 1 1 auto;
    min-height: 12px;
  }
</style>
