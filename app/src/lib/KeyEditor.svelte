<script lang="ts">
  import { pickImage } from './api';
  import { hotkeyFrom } from './hotkey';
  import { iconUrl } from './icons.svelte';
  import { ACTION_TYPES, MEDIA_KEYS, blankKey, newAction, position, type Action, type Key } from './types';

  let {
    keys = $bindable(),
    number,
    inherited = null,
    inRule,
    glyphs,
  }: {
    keys: Record<string, Key>;
    number: number;
    /** The default layout's key under a rule. */
    inherited?: Key | null;
    inRule: boolean;
    glyphs: string[];
  } = $props();

  const id = $derived(String(number));
  const key = $derived(keys[id]);
  const SWATCHES = ['#24262B', '#3A2A12', '#1F3A2A', '#12303A', '#2A1F3A', '#3A1616'];

  let recording = $state(false);
  let pickError = $state<string | null>(null);

  function create() {
    keys[id] = inherited ? { ...$state.snapshot(inherited), front: false } : blankKey();
  }

  function remove() {
    delete keys[id];
  }

  function setType(type: string) {
    keys[id].action = type ? newAction(type as Action['type']) : null;
  }

  async function chooseImage() {
    pickError = null;
    try {
      const path = await pickImage();
      if (path) keys[id].icon = path;
    } catch (e) {
      pickError = String(e);
    }
  }

  function record() {
    const action = keys[id]?.action;
    if (action?.type !== 'hotkey') return;
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
      action.keys = combo;
      stop();
    };
    const stop = () => {
      recording = false;
      window.removeEventListener('keydown', onKey, true);
    };
    window.addEventListener('keydown', onKey, true);
  }
</script>

<div class="head">
  <div class="muted small">{position(number)}</div>
  <h2>Tecla {number}</h2>
</div>

{#if !key}
  {#if inRule}
    <div class="notice">
      <div class="muted small">Nesta regra, a tecla continua com o padrão</div>
      <div>{inherited?.label || (inherited ? 'Sem texto' : 'Vazia')}</div>
    </div>
    <button type="button" class="primary" onclick={create}>Sobrescrever nesta regra</button>
  {:else}
    <div class="notice"><div>Tecla vazia</div></div>
    <button type="button" class="primary" onclick={create}>Configurar tecla</button>
  {/if}
{:else}
  <div class="field">
    <span class="name">Ícone</span>
    <div class="glyphs">
      {#each glyphs as glyph (glyph)}
        {@const url = iconUrl(glyph, key.color)}
        <button
          type="button"
          class="glyph"
          class:on={key.icon === glyph}
          aria-label={`Ícone ${glyph}`}
          aria-pressed={key.icon === glyph}
          onclick={() => (key.icon = glyph)}
        >
          {#if url}<img src={url} alt="" />{/if}
        </button>
      {/each}
    </div>
    <div class="row">
      <button type="button" class="secondary small-btn" onclick={chooseImage}>Usar imagem PNG…</button>
      <button type="button" class="secondary small-btn" aria-pressed={!key.icon} onclick={() => (key.icon = null)}>Só a cor</button>
    </div>
    {#if key.icon && !glyphs.includes(key.icon)}<span class="muted small mono">{key.icon}</span>{/if}
    {#if pickError}<span class="error small">{pickError}</span>{/if}
  </div>

  <div class="field">
    <label class="name" for="key-label">Texto na tecla</label>
    <input id="key-label" type="text" bind:value={key.label} placeholder="Ex.: Mute" />
  </div>

  <div class="field">
    <span class="name">Cor de fundo</span>
    <div class="row">
      {#each SWATCHES as swatch (swatch)}
        <button
          type="button"
          class="swatch"
          style:background={swatch}
          aria-label={`Cor ${swatch}`}
          aria-pressed={key.color.toUpperCase() === swatch}
          onclick={() => (key.color = swatch)}
        ></button>
      {/each}
      <input type="color" aria-label="Outra cor" bind:value={key.color} />
    </div>
  </div>

  <div class="field">
    <label class="name" for="key-action">Ação</label>
    <select id="key-action" value={key.action?.type ?? ''} onchange={(e) => setType(e.currentTarget.value)}>
      <option value="">Nenhuma</option>
      {#each ACTION_TYPES as type (type.value)}<option value={type.value}>{type.label}</option>{/each}
    </select>
  </div>

  {#if key.action?.type === 'hotkey'}
    <div class="field">
      <label class="name" for="key-hotkey">Teclas</label>
      <div class="row">
        <input id="key-hotkey" class="mono grow" type="text" bind:value={key.action.keys} placeholder="Ex.: Ctrl+Shift+M" />
        <button type="button" class="secondary" class:recording onclick={record} disabled={recording}>
          {recording ? 'Aperte…' : 'Gravar'}
        </button>
      </div>
      {#if recording}<span class="muted small">Aperte a combinação (Esc cancela).</span>{/if}
      <span class="muted small">Combinações com Win (Win+G, Win+D…) o Windows não deixa gravar: digite no campo, como Win+G.</span>
    </div>
  {:else if key.action?.type === 'open'}
    <div class="field">
      <label class="name" for="key-target">App, arquivo, pasta ou endereço</label>
      <input id="key-target" class="mono" type="text" bind:value={key.action.target} placeholder="Ex.: spotify.exe ou https://…" />
      <label class="name" for="key-args">Argumentos (opcional)</label>
      <input id="key-args" class="mono" type="text" bind:value={key.action.args} />
    </div>
  {:else if key.action?.type === 'command'}
    <div class="field">
      <label class="name" for="key-command">Comando (roda no cmd, sem janela)</label>
      <input id="key-command" class="mono" type="text" bind:value={key.action.command} />
    </div>
  {:else if key.action?.type === 'text'}
    <div class="field">
      <label class="name" for="key-text">Texto a digitar</label>
      <textarea id="key-text" rows="3" bind:value={key.action.text}></textarea>
    </div>
  {:else if key.action?.type === 'media'}
    <div class="field">
      <label class="name" for="key-media">Comando de mídia</label>
      <select id="key-media" bind:value={key.action.key}>
        {#each MEDIA_KEYS as media (media.value)}<option value={media.value}>{media.label}</option>{/each}
      </select>
    </div>
  {/if}

  {#if inRule}
    <label class="check">
      <input type="checkbox" bind:checked={key.front} />
      <span>
        <span>Trazer o app ou a aba para a frente antes</span>
        <span class="muted small">Atalhos só chegam na janela da frente. Depois o foco volta para onde estava.</span>
      </span>
    </label>
  {/if}

  <button type="button" class="secondary" onclick={remove}>{inRule ? 'Voltar ao padrão nesta regra' : 'Esvaziar tecla'}</button>
{/if}

<style>
  .head h2 {
    font-size: 18px;
    font-weight: 600;
  }
  .notice {
    padding: 14px;
    border: 1px dashed #4a4e57;
    border-radius: 12px;
    background: var(--surface);
  }
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
    flex-wrap: wrap;
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
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 8px 10px;
    color: var(--text);
    font: inherit;
  }
  .row input[type='text'] {
    width: auto;
  }
  textarea {
    resize: vertical;
  }
  .glyphs {
    display: grid;
    grid-template-columns: repeat(6, minmax(0, 1fr));
    gap: 6px;
  }
  .glyph {
    aspect-ratio: 1 / 1;
    padding: 0;
    border-radius: 8px;
    border: 1px solid #2e3036;
    background: var(--surface);
    overflow: hidden;
  }
  .glyph img {
    width: 100%;
    height: 100%;
    display: block;
  }
  .glyph.on {
    border: 2px solid var(--accent);
  }
  .swatch {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    border: 1px solid var(--line);
    padding: 0;
  }
  .swatch[aria-pressed='true'] {
    border: 2px solid var(--accent);
  }
  input[type='color'] {
    width: 36px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: transparent;
  }
  .small-btn {
    padding: 6px 10px;
    font-size: 13px;
  }
  .small-btn[aria-pressed='true'] {
    border-color: var(--accent);
  }
  .recording {
    border-color: var(--accent);
    color: var(--accent);
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 12px;
    border: 1px solid #2e3036;
    border-radius: 12px;
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
  .error {
    color: #ff9b8f;
  }
</style>
