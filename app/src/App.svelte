<script lang="ts">
  import { onMount } from 'svelte';
  import { getStatus, glyphs as loadGlyphs, onStatus, openConfig } from './lib/api';
  import Banners from './lib/Banners.svelte';
  import { ConfigEditing } from './lib/editing.svelte';
  import LayerEditor from './lib/LayerEditor.svelte';
  import { moveItem } from './lib/list';
  import Live from './lib/Live.svelte';
  import NewRule from './lib/NewRule.svelte';
  import RuleNav from './lib/RuleNav.svelte';
  import Settings from './lib/Settings.svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import { freeRuleName, type Glyph, type Key, type Rule, type Status, type View } from './lib/types';

  let status = $state<Status | null>(null);
  let glyphs = $state<Glyph[]>([]);
  let view = $state<View>({ kind: 'live' });
  // Raw: the editors hand over a plain copy, and pasting clones it with
  // structuredClone, which throws on the proxy a deep $state would make.
  let clipboard = $state.raw<Key | null>(null);

  const editing = new ConfigEditing(() => status?.config_revision, keepViewValid);

  onMount(() => {
    getStatus().then((s) => (status = s));
    const stopStatus = onStatus((s) => (status = s));
    loadGlyphs().then((g) => (glyphs = g));
    editing.load();
    return () => {
      stopStatus.then((unlisten) => unlisten());
    };
  });

  /** The rule being edited may be gone in another version of the config. */
  function keepViewValid() {
    if (view.kind === 'rule' && editing.config && !editing.config.rules[view.index]) view = { kind: 'live' };
  }

  function onKey(event: KeyboardEvent) {
    if (!event.ctrlKey || event.altKey) return;
    // Text fields keep their own undo.
    if ((event.target as HTMLElement | null)?.closest('input, textarea, select, [contenteditable]')) return;
    const letter = event.key.toLowerCase();
    if (letter === 'z' && !event.shiftKey) {
      event.preventDefault();
      editing.undo();
    } else if (letter === 'y' || (letter === 'z' && event.shiftKey)) {
      event.preventDefault();
      editing.redo();
    }
  }

  function createRule(rule: Rule) {
    if (!editing.config) return;
    editing.config.rules.push({ ...rule, name: freeRuleName(editing.config.rules, rule.name) });
    view = { kind: 'rule', index: editing.config.rules.length - 1 };
  }

  function moveRule(from: number, to: number) {
    if (!editing.config || from === to) return;
    const rules = editing.config.rules;
    const editingRule = view.kind === 'rule' ? rules[view.index] : null;
    moveItem(rules, from, to);
    if (editingRule) view = { kind: 'rule', index: rules.indexOf(editingRule) };
  }
</script>

<svelte:window onkeydown={onKey} />

{#if status}
  <div class="shell">
    <TitleBar
      {status}
      saveState={editing.saveState}
      canUndo={editing.canUndo}
      canRedo={editing.canRedo}
      onundo={() => editing.undo()}
      onredo={() => editing.redo()}
    />
    <Banners {status} saveError={editing.saveError} />

    <div class="body">
      <RuleNav bind:view rules={editing.config?.rules ?? null} statuses={status.rules} onmove={moveRule} />

      {#if view.kind === 'live'}
        <Live {status} />
      {:else if !editing.config}
        <div class="page">
          <h1>Não deu para abrir a configuração</h1>
          <p class="muted">{editing.loadError ?? 'Carregando…'}</p>
          <div><button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button></div>
        </div>
      {:else if view.kind === 'default'}
        <LayerEditor
          bind:config={editing.config}
          index={null}
          active={false}
          {glyphs}
          edgePages={status.edge_pages}
          {clipboard}
          oncopy={(key) => (clipboard = key)}
          onmoved={() => {}}
          ondeleted={() => {}}
        />
      {:else if view.kind === 'rule' && editing.config.rules[view.index]}
        {#key view.index}
          <LayerEditor
            bind:config={editing.config}
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
        <Settings bind:config={editing.config} configPath={status.config_path} />
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
  .body {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
  }
  .page {
    flex: 1 1 auto;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
