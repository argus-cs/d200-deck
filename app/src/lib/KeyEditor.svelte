<script lang="ts">
  import ActionEditor from './ActionEditor.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import Icon from './Icon.svelte';
  import IconPicker from './IconPicker.svelte';
  import {
    ICON_COLOR,
    SETTINGS,
    blankKey,
    position,
    secondFace,
    settingInfo,
    settingLook,
    type Action,
    type EdgePage,
    type Glyph,
    type Key,
    type KeyFace,
    type LabelStyle,
  } from './types';

  let {
    keys = $bindable(),
    number,
    inherited = null,
    inRule,
    inFolder = false,
    glyphs,
    edgePages,
    labelStyle,
    clipboard,
    oncopy,
    onopenfolder,
  }: {
    keys: Record<string, Key>;
    number: number;
    /** The default layout's key under a rule. */
    inherited?: Key | null;
    inRule: boolean;
    /** Editing a key inside a folder: no folder in a folder. */
    inFolder?: boolean;
    glyphs: Glyph[];
    edgePages: EdgePage[];
    /** Where the default text color and size come from. */
    labelStyle: LabelStyle;
    clipboard: Key | null;
    oncopy: (key: Key) => void;
    /** Show this key's folder in the editor. */
    onopenfolder?: (n: number) => void;
  } = $props();

  /** Looks the app suggested for setting keys: those may be replaced by the next suggestion. */
  const SUGGESTED_LABELS = new Set(SETTINGS.flatMap((s) => [s.short, ...(s.choices ?? []).map((c) => c.short)]));
  const SUGGESTED_ICONS = new Set(SETTINGS.map((s) => s.icon));
  const SUGGESTED_OFF = new Set(SETTINGS.flatMap((s) => (s.off ? [s.off.icon] : [])));
  const SECOND_COLOR = '#3A1616';

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
  const folderSize = $derived(key?.folder ? Object.keys(key.folder.keys).length : 0);
  /** The Windows setting whose state this key shows, if any. */
  const shown = $derived(key?.action?.type === 'system' ? settingInfo(key.action.setting) : null);
  const followsSetting = $derived(!!shown && shown.kind !== 'once');

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
    if (!clipboard) return;
    const pasted: Key = { ...structuredClone(clipboard), front: inRule ? (clipboard.front ?? false) : false };
    // A folder can't hold another one.
    if (inFolder && pasted.folder) pasted.folder = null;
    keys[id] = pasted;
  }

  function setTwoState(on: boolean) {
    if (!on) {
      keys[id].toggle = null;
    } else if (keys[id].action?.type === 'system') {
      const off = settingLook(keys[id].action as Extract<Action, { type: 'system' }>).off;
      keys[id].toggle = off ? { label: off.label, icon: off.icon, color: SECOND_COLOR, action: null } : secondFace($state.snapshot(keys[id]));
    } else {
      keys[id].toggle = secondFace($state.snapshot(keys[id]));
    }
  }

  /** A folder runs no action of its own. */
  function setFolder(on: boolean) {
    const k = keys[id];
    if (on) {
      k.folder = { keys: {} };
      k.action = null;
      k.toggle = null;
      k.front = false;
      if (!k.icon) k.icon = 'folder';
    } else {
      k.folder = null;
    }
  }

  /** Gives a key the look of the Windows setting just picked, keeping what the person chose themselves. */
  function suggest(action: Extract<Action, { type: 'system' }>) {
    const k = keys[id];
    const look = settingLook(action);
    if (!k.label || SUGGESTED_LABELS.has(k.label)) k.label = look.label;
    if (!k.icon || SUGGESTED_ICONS.has(k.icon)) k.icon = look.icon;
    const suggestedSecond = !k.toggle || (k.toggle.icon && SUGGESTED_OFF.has(k.toggle.icon));
    if (suggestedSecond) {
      k.toggle = look.off ? { label: look.off.label, icon: look.off.icon, color: SECOND_COLOR, action: null } : null;
    }
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

  {#if !inFolder}
    <div class="field">
      <span class="name">Ao apertar</span>
      <div class="segmented" role="group" aria-label="Ao apertar">
        <button type="button" aria-pressed={!key.folder} onclick={() => setFolder(false)}>Executa uma ação</button>
        <button type="button" aria-pressed={!!key.folder} onclick={() => setFolder(true)}>Abre uma pasta</button>
      </div>
    </div>
  {/if}

  {#if key.folder}
    <div class="notice folder">
      <div>
        <div>{folderSize === 0 ? 'Pasta vazia' : `${folderSize} ${folderSize === 1 ? 'tecla' : 'teclas'} na pasta`}</div>
        <div class="muted small">No D200, a pasta ocupa as teclas 2 a 13 e a tecla 1 volta.</div>
      </div>
      <button type="button" class="primary" onclick={() => onopenfolder?.(number)}>Editar a pasta</button>
    </div>
    <label class="check">
      <input
        type="checkbox"
        checked={!!key.folder.stay}
        onchange={(e) => {
          if (key.folder) key.folder.stay = e.currentTarget.checked || undefined;
        }}
      />
      <span>
        <span>Continuar aberta depois de usar uma tecla</span>
        <span class="muted small">
          Para teclas usadas em sequência, como volume. Sem isso, a pasta fecha assim que uma tecla dela é usada. Ela também fecha sozinha
          depois de 30 segundos sem toque.
        </span>
      </span>
    </label>
  {:else}
    <ActionEditor bind:action={key.action} id="key" {edgePages} onpick={suggest} />

    <label class="check">
      <input type="checkbox" checked={!!key.toggle} onchange={(e) => setTwoState(e.currentTarget.checked)} />
      <span>
        {#if followsSetting && shown?.kind === 'choice'}
          <span>Outro visual quando não estiver em uso</span>
          <span class="muted small">Sem ele, a tecla só escurece enquanto outra opção estiver em uso.</span>
        {:else if followsSetting}
          <span>Outro visual quando estiver desligado</span>
          <span class="muted small">A tecla segue o estado real do Windows. Sem o segundo visual, ela só escurece quando desligado.</span>
        {:else}
          <span>Tecla de dois estados</span>
          <span class="muted small">Cada toque executa a ação e alterna o visual, como microfone ligado e mutado.</span>
        {/if}
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
        {#if !followsSetting}
          <p class="muted small">
            O app não sabe se o outro programa mudou de estado sozinho (por exemplo, se você mutar pelo mouse). Nesse caso, um toque a mais
            acerta o visual.
          </p>
        {/if}
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
  .folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    border-style: solid;
  }
  .folder .primary {
    flex: none;
  }
  .segmented {
    display: flex;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 11px;
  }
  .segmented button {
    flex: 1 1 0;
    border: 0;
    border-radius: 8px;
    padding: 6px 10px;
    background: transparent;
    color: var(--soft);
    font-weight: 500;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--ink);
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
