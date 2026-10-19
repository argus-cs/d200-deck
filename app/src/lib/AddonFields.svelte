<script lang="ts">
  import { untrack } from 'svelte';
  import { addonOptions } from './api';
  import { addonProblem, fieldValue, findAction, navigation, type AddonActionRef } from './addons.svelte';
  import Icon from './Icon.svelte';
  import type { AddonChoice, AddonField, AddonState } from './types';

  let {
    action = $bindable(),
    id,
    onchoice,
  }: {
    action: AddonActionRef;
    /** Prefix for the field ids, so two editors can share a page. */
    id: string;
    /** A choice was picked in a list, so the key can take its text and image. */
    onchoice?: (choice: AddonChoice) => void;
  } = $props();

  const STATE_TEXT: Record<AddonState, string> = {
    running: 'rodando',
    restarting: 'abrindo de novo',
    stopped: 'parado',
    off: 'desligado',
    invalid: 'com erro',
  };

  const found = $derived(findAction(action));
  const problem = $derived(addonProblem(action));
  /** Changes only when the lists should load again: another action, or the addon starting. */
  const listsFor = $derived(found?.addon.state === 'running' ? `${action.addon}:${action.action}` : null);

  /** The choices the addon gave for each dynamic list, by field id. */
  let lists = $state<Record<string, AddonChoice[]>>({});
  let loading = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string | null>>({});

  async function load(field: AddonField) {
    // The answer belongs to the action it was asked for, even if another shows by then.
    const asked = listsFor;
    loading[field.id] = true;
    errors[field.id] = null;
    try {
      const choices = await addonOptions(action.addon, action.action, field.id, $state.snapshot(action.settings));
      if (asked === listsFor) lists[field.id] = choices;
    } catch (e) {
      if (asked === listsFor) errors[field.id] = String(e);
    } finally {
      loading[field.id] = false;
    }
  }

  // The lists the addon fills load when it runs; nothing here writes to the config.
  $effect(() => {
    if (!listsFor) return;
    untrack(() => {
      lists = {};
      for (const field of found?.def.fields ?? []) if (field.type === 'select' && field.dynamic) load(field);
    });
  });

  function set(field: AddonField, value: unknown) {
    action.settings[field.id] = value;
  }

  function setNumber(field: AddonField, text: string) {
    const n = Number.parseFloat(text);
    if (Number.isFinite(n)) action.settings[field.id] = n;
    else delete action.settings[field.id];
  }

  function pick(field: AddonField, value: string, choices: AddonChoice[]) {
    if (!value) {
      delete action.settings[field.id];
      return;
    }
    set(field, value);
    const choice = choices.find((c) => c.value === value);
    if (choice) onchoice?.(choice);
  }
</script>

{#if found}
  <span class="status muted small">
    <span class="dot {found.addon.state}"></span>Addon {found.addon.name}
    {found.addon.version} · {STATE_TEXT[found.addon.state]}
  </span>
  {#if found.def.description}<span class="muted small">{found.def.description}</span>{/if}
{/if}

{#if problem}
  <div class="problem small">
    <span class="row-icon"><Icon name="alert" size={16} /><span>{problem}</span></span>
    <button type="button" class="secondary small-btn" onclick={() => navigation.openAddons()}>Ver em Addons</button>
  </div>
{/if}

{#if found}
  {#each found.def.fields as field (field.id)}
    {@const fid = `${id}-addon-${field.id}`}
    {#if field.type === 'checkbox'}
      <label class="check">
        <input type="checkbox" checked={!!fieldValue(field, action.settings)} onchange={(e) => set(field, e.currentTarget.checked)} />
        <span>
          <span>{field.label}</span>
          {#if field.hint}<span class="muted small">{field.hint}</span>{/if}
        </span>
      </label>
    {:else}
      <div class="field">
        <label class="name" for={fid}>{field.label}</label>
        {#if field.type === 'text'}
          <input
            id={fid}
            type="text"
            value={String(fieldValue(field, action.settings) ?? '')}
            placeholder={field.placeholder ?? ''}
            oninput={(e) => set(field, e.currentTarget.value)}
          />
        {:else if field.type === 'number'}
          <input
            id={fid}
            type="number"
            min={field.min ?? undefined}
            max={field.max ?? undefined}
            value={fieldValue(field, action.settings) as number | string}
            placeholder={field.placeholder ?? ''}
            oninput={(e) => setNumber(field, e.currentTarget.value)}
          />
        {:else}
          {@const choices = field.dynamic ? (lists[field.id] ?? []) : (field.options ?? [])}
          {@const current = String(fieldValue(field, action.settings) ?? '')}
          <div class="row">
            <select id={fid} class="grow" value={current} onchange={(e) => pick(field, e.currentTarget.value, choices)}>
              <option value="">{loading[field.id] ? 'Carregando…' : 'Escolha…'}</option>
              <!-- A value saved before the list loads, or that left it, still shows. -->
              {#if current && !choices.some((choice) => choice.value === current)}<option value={current}>{current}</option>{/if}
              {#each choices as choice (choice.value)}<option value={choice.value}>{choice.label}</option>{/each}
            </select>
            {#if field.dynamic}
              <button
                type="button"
                class="icon-btn"
                title="Atualizar a lista"
                aria-label="Atualizar a lista"
                disabled={!listsFor || loading[field.id]}
                onclick={() => load(field)}
              >
                <Icon name="refresh" size={16} />
              </button>
            {/if}
          </div>
          {#if field.dynamic && !listsFor}
            <span class="muted small">A lista vem do addon, que não está rodando. O valor salvo continua valendo.</span>
          {/if}
          {#if errors[field.id]}<span class="error-text small">{errors[field.id]}</span>{/if}
        {/if}
        {#if field.hint}<span class="muted small">{field.hint}</span>{/if}
      </div>
    {/if}
  {/each}
{:else if Object.keys(action.settings ?? {}).length > 0}
  <div class="field">
    <span class="name">Configurações guardadas</span>
    <span class="mono muted small saved">{JSON.stringify(action.settings)}</span>
  </div>
{/if}

<style>
  .status {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    border: 1.5px solid var(--muted);
    flex: none;
  }
  .dot.running {
    border-color: var(--accent);
    background: var(--accent);
  }
  .dot.stopped,
  .dot.restarting {
    border-color: var(--error-text);
    background: var(--error-text);
  }
  .problem {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--error);
    border: 1px solid var(--error-line);
    color: var(--error-text);
  }
  .row-icon {
    display: flex;
    gap: 8px;
    align-items: flex-start;
  }
  .row-icon :global(svg) {
    margin-top: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1 1 auto;
    min-width: 0;
  }
  input[type='text'],
  input[type='number'],
  select {
    width: 100%;
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px;
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    background: var(--surface);
  }
  .check > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .check input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  .saved {
    overflow-wrap: anywhere;
  }
</style>
