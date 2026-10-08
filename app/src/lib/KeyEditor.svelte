<script lang="ts">
  import ActionEditor from './ActionEditor.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import Icon from './Icon.svelte';
  import IconPicker from './IconPicker.svelte';
  import { ICON_COLOR, blankKey, position, secondFace, type EdgePage, type Glyph, type Key, type KeyFace, type LabelStyle } from './types';

  let {
    keys = $bindable(),
    number,
    inherited = null,
    inRule,
    glyphs,
    edgePages,
    labelStyle,
    clipboard,
    oncopy,
  }: {
    keys: Record<string, Key>;
    number: number;
    /** The default layout's key under a rule. */
    inherited?: Key | null;
    inRule: boolean;
    glyphs: Glyph[];
    edgePages: EdgePage[];
    /** Where the default text color and size come from. */
    labelStyle: LabelStyle;
    clipboard: Key | null;
    oncopy: (key: Key) => void;
  } = $props();

  type Paint = 'color' | 'icon_color' | 'text_color' | 'border';

  function paint(face: KeyFace | null | undefined, field: Paint, color: string | null) {
    if (!face) return;
    if (field === 'color') {
      if (color) face.color = color;
    } else {
      face[field] = color;
    }
  }

  const id = $derived(String(number));
  const key = $derived(keys[id]);

  function create() {
    keys[id] = inherited ? { ...$state.snapshot(inherited), front: false } : blankKey();
  }

  function remove() {
    delete keys[id];
  }

  function copy() {
    if (key) oncopy($state.snapshot(key));
  }

  function paste() {
    if (clipboard) keys[id] = { ...structuredClone(clipboard), front: inRule ? (clipboard.front ?? false) : false };
  }

  function setTwoState(on: boolean) {
    keys[id].toggle = on ? secondFace($state.snapshot(keys[id])) : null;
  }
</script>

<div class="head">
  <div>
    <div class="muted small">{position(number)}</div>
    <h2>Tecla {number}</h2>
  </div>
  <div class="clip">
    <button type="button" class="icon-btn" title="Copiar tecla (Ctrl+C)" aria-label="Copiar tecla" disabled={!key} onclick={copy}>
      <Icon name="copy" size={16} />
    </button>
    <button type="button" class="icon-btn" title="Colar tecla (Ctrl+V)" aria-label="Colar tecla" disabled={!clipboard} onclick={paste}>
      <Icon name="paste" size={16} />
    </button>
  </div>
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
    <IconPicker icon={key.icon} color={key.color} iconColor={key.icon_color} {glyphs} {edgePages} onchange={(icon) => (key.icon = icon)} />
  </div>

  <div class="field">
    <label class="name" for="key-label">Texto na tecla</label>
    <input id="key-label" type="text" bind:value={key.label} placeholder="Ex.: Mute" />
  </div>

  <div class="field">
    <span class="name">Cores</span>
    <ColorPicker label="Fundo" value={key.color} onchange={(c) => paint(key, 'color', c)} />
    <ColorPicker label="Ícone" value={key.icon_color} placeholder={ICON_COLOR} allowNone noneLabel="Padrão" onchange={(c) => paint(key, 'icon_color', c)} />
    <ColorPicker
      label="Texto"
      value={key.text_color}
      placeholder={`#${labelStyle.color}`}
      allowNone
      noneLabel="Padrão dos Ajustes"
      onchange={(c) => paint(key, 'text_color', c)}
    />
    <ColorPicker label="Borda" value={key.border} allowNone noneLabel="Sem borda" onchange={(c) => paint(key, 'border', c)} />
  </div>

  <ActionEditor bind:action={key.action} id="key" {edgePages} />

  <label class="check">
    <input type="checkbox" checked={!!key.toggle} onchange={(e) => setTwoState(e.currentTarget.checked)} />
    <span>
      <span>Tecla de dois estados</span>
      <span class="muted small">Cada toque executa a ação e alterna o visual, como microfone ligado e mutado.</span>
    </span>
  </label>

  {#if key.toggle}
    <section class="second">
      <h3>Segundo estado</h3>
      <div class="field">
        <span class="name">Ícone</span>
        <IconPicker
          icon={key.toggle.icon}
          color={key.toggle.color}
          iconColor={key.toggle.icon_color}
          {glyphs}
          {edgePages}
          onchange={(icon) => {
            if (key.toggle) key.toggle.icon = icon;
          }}
        />
      </div>
      <div class="field">
        <label class="name" for="second-label">Texto na tecla</label>
        <input id="second-label" type="text" bind:value={key.toggle.label} placeholder="Ex.: Mutado" />
      </div>
      <div class="field">
        <span class="name">Cores</span>
        <ColorPicker label="Fundo" value={key.toggle.color} onchange={(c) => paint(key.toggle, 'color', c)} />
        <ColorPicker
          label="Ícone"
          value={key.toggle.icon_color}
          placeholder={ICON_COLOR}
          allowNone
          noneLabel="Padrão"
          onchange={(c) => paint(key.toggle, 'icon_color', c)}
        />
        <ColorPicker
          label="Texto"
          value={key.toggle.text_color}
          placeholder={`#${labelStyle.color}`}
          allowNone
          noneLabel="Padrão dos Ajustes"
          onchange={(c) => paint(key.toggle, 'text_color', c)}
        />
        <ColorPicker label="Borda" value={key.toggle.border} allowNone noneLabel="Sem borda" onchange={(c) => paint(key.toggle, 'border', c)} />
      </div>
      <ActionEditor bind:action={key.toggle.action} id="second" noneLabel="A mesma do primeiro estado" {edgePages} />
      <p class="muted small">
        O app não sabe se o outro programa mudou de estado sozinho (por exemplo, se você mutar pelo mouse). Nesse caso, um toque a mais
        acerta o visual.
      </p>
    </section>
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
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .head h2 {
    font-size: 18px;
    font-weight: 600;
  }
  .clip {
    display: flex;
    gap: 6px;
  }
  .icon-btn {
    width: 34px;
    height: 34px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 9px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--soft);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--hover);
  }
  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .notice {
    padding: 14px;
    border: 1px dashed var(--line-strong);
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
  input[type='text'] {
    width: 100%;
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 12px;
    border: 1px solid var(--line-soft);
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
  .second {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 14px;
    border: 1px solid var(--accent);
    border-radius: 12px;
  }
  .second h3 {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--accent-text);
  }
</style>
