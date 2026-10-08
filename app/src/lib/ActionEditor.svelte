<script lang="ts">
  import { pickFile, runningApps } from './api';
  import { hotkeyFrom } from './hotkey';
  import { ACTION_TYPES, MEDIA_KEYS, newAction, opensApp, type Action, type EdgePage, type RunningApp } from './types';

  let {
    action = $bindable(),
    id,
    noneLabel = 'Nenhuma',
    edgePages = [],
  }: {
    action: Action | null | undefined;
    /** Prefix for the field ids, so two editors can share a page. */
    id: string;
    noneLabel?: string;
    /** Open Edge pages, offered as quick targets. */
    edgePages?: EdgePage[];
  } = $props();

  let recording = $state(false);
  let panel = $state<'none' | 'apps' | 'sites'>('none');
  let apps = $state<RunningApp[]>([]);
  let pickError = $state<string | null>(null);

  async function togglePanel(next: 'apps' | 'sites') {
    panel = panel === next ? 'none' : next;
    if (panel === 'apps') apps = (await runningApps()).filter((a) => a.exe.toLowerCase() !== 'd200-deck.exe');
  }

  /** Store apps open by name (through their alias); started from their own
   * folder they break. Everything else opens by full path. */
  const targetFor = (app: RunningApp) => (/\\WindowsApps\\/i.test(app.path) ? app.exe : app.path);

  function setTarget(target: string) {
    if (action?.type !== 'open') return;
    action.target = target;
    action.args = null;
    panel = 'none';
  }

  /** On is the default, so it is left out of config.json. */
  function setWatch(on: boolean) {
    if (action?.type === 'open') action.watch = on ? undefined : false;
  }

  async function chooseFile() {
    pickError = null;
    try {
      const path = await pickFile();
      if (path) setTarget(path);
    } catch (e) {
      pickError = String(e);
    }
  }

  function setType(type: string) {
    action = type ? newAction(type as Action['type']) : null;
  }

  function record() {
    const current = action;
    if (current?.type !== 'hotkey') return;
    recording = true;
    const onKey = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.code === 'Escape' && !event.ctrlKey && !event.altKey && !event.shiftKey) {
        stop();
        return;
      }
      const combo = hotkeyFrom(event);
      if (!combo) return;
      current.keys = combo;
      stop();
    };
    const stop = () => {
      recording = false;
      window.removeEventListener('keydown', onKey, true);
    };
    window.addEventListener('keydown', onKey, true);
  }
</script>

<div class="field">
  <label class="name" for={`${id}-type`}>Ação</label>
  <select id={`${id}-type`} value={action?.type ?? ''} onchange={(e) => setType(e.currentTarget.value)}>
    <option value="">{noneLabel}</option>
    {#each ACTION_TYPES as type (type.value)}<option value={type.value}>{type.label}</option>{/each}
  </select>
</div>

{#if action?.type === 'hotkey'}
  <div class="field">
    <label class="name" for={`${id}-hotkey`}>Teclas</label>
    <div class="row">
      <input id={`${id}-hotkey`} class="mono grow" type="text" bind:value={action.keys} placeholder="Ex.: Ctrl+Shift+M" />
      <button type="button" class="secondary" class:recording onclick={record} disabled={recording}>
        {recording ? 'Aperte…' : 'Gravar'}
      </button>
    </div>
    {#if recording}<span class="muted small">Aperte a combinação (Esc cancela).</span>{/if}
    <span class="muted small">Combinações com Win (Win+G, Win+D…) o Windows não deixa gravar: digite no campo, como Win+G.</span>
  </div>
{:else if action?.type === 'open'}
  <div class="field">
    <label class="name" for={`${id}-target`}>App, arquivo, pasta ou endereço</label>
    <input id={`${id}-target`} class="mono" type="text" bind:value={action.target} placeholder="Ex.: spotify.exe ou https://…" />
    <div class="quick">
      <button type="button" class="secondary small-btn" aria-expanded={panel === 'apps'} onclick={() => togglePanel('apps')}>Apps abertos</button>
      <button type="button" class="secondary small-btn" aria-expanded={panel === 'sites'} onclick={() => togglePanel('sites')}>Sites abertos</button>
      <button type="button" class="secondary small-btn" onclick={chooseFile}>Escolher arquivo…</button>
    </div>
    {#if panel === 'apps'}
      <ul class="choices scroll">
        {#each apps as app (app.path)}
          <li>
            <button type="button" class="choice" onclick={() => setTarget(targetFor(app))}>
              <span class="ellipsis">{app.title}</span>
              <span class="mono muted small ellipsis">{targetFor(app)}</span>
            </button>
          </li>
        {/each}
        {#if apps.length === 0}<li class="muted small">Nenhum app com janela aberta.</li>{/if}
      </ul>
    {:else if panel === 'sites'}
      <ul class="choices scroll">
        {#each edgePages as page (page.host)}
          <li>
            <button type="button" class="choice" onclick={() => setTarget(page.url)}>
              <span class="ellipsis">{page.title || page.host}</span>
              <span class="mono muted small ellipsis">{page.url}</span>
            </button>
          </li>
        {/each}
        {#if edgePages.length === 0}<li class="muted small">Abra o site no Edge (com a extensão conectada) e ele aparece aqui.</li>{/if}
      </ul>
    {/if}
    {#if pickError}<span class="error-text small">{pickError}</span>{/if}
    <label class="name" for={`${id}-args`}>Argumentos (opcional)</label>
    <input id={`${id}-args`} class="mono" type="text" bind:value={action.args} />
    {#if opensApp(action.target)}
      <label class="check">
        <input type="checkbox" checked={action.watch !== false} onchange={(e) => setWatch(e.currentTarget.checked)} />
        <span>
          <span>Escurecer a tecla enquanto o app estiver fechado</span>
          <span class="muted small">Quando o app abre, a tecla volta à cor normal.</span>
        </span>
      </label>
    {/if}
  </div>
{:else if action?.type === 'command'}
  <div class="field">
    <label class="name" for={`${id}-command`}>Comando (roda no cmd, sem janela)</label>
    <input id={`${id}-command`} class="mono" type="text" bind:value={action.command} />
  </div>
{:else if action?.type === 'text'}
  <div class="field">
    <label class="name" for={`${id}-text`}>Texto a digitar</label>
    <textarea id={`${id}-text`} rows="3" bind:value={action.text}></textarea>
  </div>
{:else if action?.type === 'media'}
  <div class="field">
    <label class="name" for={`${id}-media`}>Comando de mídia</label>
    <select id={`${id}-media`} bind:value={action.key}>
      {#each MEDIA_KEYS as media (media.value)}<option value={media.value}>{media.label}</option>{/each}
    </select>
  </div>
{/if}

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
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
  select,
  textarea {
    width: 100%;
  }
  .row input[type='text'] {
    width: auto;
  }
  textarea {
    resize: vertical;
  }
  .recording {
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .quick {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
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
  .small-btn {
    padding: 5px 10px;
    font-size: 13px;
  }
  .small-btn[aria-expanded='true'] {
    border-color: var(--accent);
  }
  .choices {
    list-style: none;
    margin: 0;
    padding: 8px;
    max-height: 220px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    background: var(--surface);
  }
  .choice {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
    padding: 7px 9px;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: transparent;
    text-align: left;
    min-width: 0;
  }
  .choice:hover {
    background: var(--hover);
  }
  .choice span {
    max-width: 100%;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
