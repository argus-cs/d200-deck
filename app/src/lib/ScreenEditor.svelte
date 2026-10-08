<script lang="ts">
  import ActionEditor from './ActionEditor.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import Icon from './Icon.svelte';
  import IconPicker from './IconPicker.svelte';
  import { screenUrl } from './icons.svelte';
  import {
    SCREEN_TYPES,
    newContent,
    screenLabel,
    type EdgePage,
    type Glyph,
    type Screen,
    type ScreenContent,
    type ScreenHold,
  } from './types';

  let {
    screen = $bindable(),
    inRule,
    inherited,
    glyphs,
    edgePages,
  }: {
    /** This layer's visor; empty in a rule that keeps the default one. */
    screen: Screen | null | undefined;
    inRule: boolean;
    /** The default visor (what a rule falls back to, or the default before it is saved). */
    inherited: Screen;
    glyphs: Glyph[];
    edgePages: EdgePage[];
  } = $props();

  // The default layout always has a visor of its own: start from the current one.
  $effect(() => {
    if (!inRule && !screen) screen = $state.snapshot(inherited);
  });

  type HoldKind = 'none' | ScreenHold['type'];
  const HOLD_KINDS: { value: HoldKind; label: string; hint: string }[] = [
    { value: 'none', label: 'Nada', hint: 'Segurar conta como um toque' },
    { value: 'action', label: 'Executar uma ação', hint: 'Diferente da ação do toque' },
    { value: 'cycle', label: 'Alternar o que o visor mostra', hint: 'Cada vez que segurar, passa para o próximo da sequência' },
  ];

  /** The hold sequence's entry being edited and previewed; null: the visor's own content. */
  let editing = $state<number | null>(null);

  const shown = $derived(screen ?? inherited);
  const cycle = $derived(shown.hold?.type === 'cycle' ? shown.hold.contents : []);
  const previewed = $derived((editing !== null && cycle[editing]) || shown.content);
  const preview = $derived(screenUrl({ ...shown, content: previewed }));
  const holdKind = $derived<HoldKind>(shown.hold?.type ?? 'none');
  /** Contents the sequence doesn't have yet. */
  const addable = $derived(SCREEN_TYPES.filter((t) => t.type !== shown.content.type && !cycle.some((c) => c.type === t.type)));
  const all = $derived([shown.content, ...cycle]);
  /** The timer takes taps itself, so a tap action only matters when something else shows. */
  const tapsAction = $derived(all.some((c) => c.type !== 'timer'));

  const deviceDrawn = (content: ScreenContent) => content.type === 'device_clock' || content.type === 'device_stats';

  function setType(type: ScreenContent['type']) {
    if (screen) screen.content = newContent(type);
    editing = null;
  }

  /** Sets one option of a content (24h, seconds, CPU…). */
  function setOption(content: ScreenContent, name: string, value: boolean | number | string | null) {
    (content as Record<string, unknown>)[name] = value;
  }

  function setHold(kind: HoldKind) {
    if (!screen) return;
    editing = null;
    screen.hold = kind === 'action' ? { type: 'action', action: null } : kind === 'cycle' ? { type: 'cycle', contents: [] } : null;
  }

  function addToCycle(type: string) {
    if (screen?.hold?.type !== 'cycle' || !type) return;
    screen.hold.contents.push(newContent(type as ScreenContent['type']));
    editing = screen.hold.contents.length - 1;
  }

  function moveInCycle(index: number, by: number) {
    if (screen?.hold?.type !== 'cycle') return;
    const contents = screen.hold.contents;
    const to = index + by;
    if (to < 0 || to >= contents.length) return;
    const [content] = contents.splice(index, 1);
    contents.splice(to, 0, content);
    if (editing === index) editing = to;
    else if (editing === to) editing = index;
  }

  function removeFromCycle(index: number) {
    if (screen?.hold?.type !== 'cycle') return;
    screen.hold.contents.splice(index, 1);
    editing = null;
  }
</script>

{#snippet options(content: ScreenContent, id: string)}
  {#if content.type === 'clock' || content.type === 'clock_stats'}
    <div class="checks">
      <label><input type="checkbox" checked={content.hour24} onchange={(e) => setOption(content, 'hour24', e.currentTarget.checked)} /> 24 horas</label>
      <label><input type="checkbox" checked={content.seconds} onchange={(e) => setOption(content, 'seconds', e.currentTarget.checked)} /> Segundos</label>
      {#if content.type === 'clock'}
        <label><input type="checkbox" checked={content.date} onchange={(e) => setOption(content, 'date', e.currentTarget.checked)} /> Data</label>
      {/if}
    </div>
  {:else if content.type === 'stats'}
    <div class="checks">
      <label><input type="checkbox" checked={content.cpu} onchange={(e) => setOption(content, 'cpu', e.currentTarget.checked)} /> CPU</label>
      <label><input type="checkbox" checked={content.memory} onchange={(e) => setOption(content, 'memory', e.currentTarget.checked)} /> Memória</label>
      <label><input type="checkbox" checked={content.gpu} onchange={(e) => setOption(content, 'gpu', e.currentTarget.checked)} /> GPU</label>
      <label><input type="checkbox" checked={content.network} onchange={(e) => setOption(content, 'network', e.currentTarget.checked)} /> Rede</label>
    </div>
  {:else if content.type === 'timer'}
    {@const minutes = content.minutes}
    <div class="field">
      <div class="checks">
        <label><input type="radio" name={`${id}-timer-kind`} checked={minutes === 0} onchange={() => setOption(content, 'minutes', 0)} /> Cronômetro (conta para cima)</label>
        <label><input type="radio" name={`${id}-timer-kind`} checked={minutes > 0} onchange={() => setOption(content, 'minutes', 25)} /> Contagem regressiva</label>
      </div>
      {#if minutes > 0}
        <div class="row">
          {#each [5, 15, 25, 50] as preset (preset)}
            <button type="button" class="secondary small-btn" aria-pressed={minutes === preset} onclick={() => setOption(content, 'minutes', preset)}>
              {preset === 25 ? '25 · Pomodoro' : preset} min
            </button>
          {/each}
          <label class="minutes">
            <input
              type="number"
              min="1"
              max="600"
              value={minutes}
              onchange={(e) => setOption(content, 'minutes', Math.max(1, Math.min(600, Number(e.currentTarget.value) || 1)))}
            /> min
          </label>
        </div>
      {/if}
      <span class="muted small">No aparelho: um toque inicia e pausa; dois toques rápidos zeram.</span>
    </div>
  {:else if content.type === 'image'}
    <div class="field">
      <span class="name">Imagem</span>
      <IconPicker icon={content.icon ?? null} color={shown.background} {glyphs} {edgePages} onchange={(icon) => setOption(content, 'icon', icon)} />
    </div>
  {:else if content.type === 'text'}
    <div class="field">
      <label class="name" for={`${id}-text`}>Texto (até 3 linhas)</label>
      <textarea id={`${id}-text`} rows="3" value={content.text} oninput={(e) => setOption(content, 'text', e.currentTarget.value)}></textarea>
    </div>
  {:else if content.type === 'now_playing'}
    <p class="muted small">Mostra o que estiver tocando no Windows: Spotify, YouTube no Edge, qualquer player que aparece no controle de mídia.</p>
  {:else if id !== 'screen'}
    <p class="muted small">Sem opções: o próprio D200 desenha.</p>
  {/if}
{/snippet}

<div class="head">
  <div class="muted small">Tecla 14</div>
  <h2>Visor</h2>
</div>

<div class="preview" class:device={deviceDrawn(previewed)}>
  {#if deviceDrawn(previewed)}
    <span>{screenLabel(previewed.type)}</span>
    <span class="muted small">desenhado pelo próprio D200</span>
  {:else if preview}
    <img src={preview} alt="Prévia do visor" />
  {/if}
</div>
{#if editing !== null && cycle[editing]}
  <div class="previewing muted small">
    <span>Prévia de "{screenLabel(previewed.type)}", que aparece ao segurar.</span>
    <button type="button" class="link" onclick={() => (editing = null)}>Ver o principal</button>
  </div>
{/if}

{#if inRule && !screen}
  <div class="notice">
    <div class="muted small">Nesta regra, o visor continua com o padrão</div>
    <div>{screenLabel(inherited.content.type)}</div>
  </div>
  <button type="button" class="primary" onclick={() => (screen = $state.snapshot(inherited))}>Sobrescrever nesta regra</button>
{:else if screen}
  <div class="field">
    <span class="name">Mostrar</span>
    <div class="types" role="group" aria-label="Conteúdo do visor">
      {#each SCREEN_TYPES as type (type.type)}
        <button type="button" class="type" aria-pressed={screen.content.type === type.type} onclick={() => setType(type.type)}>
          <strong>{type.label}</strong>
          <span class="muted small">{type.hint}</span>
        </button>
      {/each}
    </div>
  </div>

  {@render options(screen.content, 'screen')}

  {#if all.some((c) => !deviceDrawn(c))}
    <div class="field">
      <span class="name">Cores</span>
      <ColorPicker label="Fundo" value={screen.background} onchange={(c) => screen && c && (screen.background = c)} />
      <ColorPicker label="Texto" value={screen.color} onchange={(c) => screen && c && (screen.color = c)} />
      <ColorPicker label="Destaque (barras, avisos)" value={screen.accent} onchange={(c) => screen && c && (screen.accent = c)} />
    </div>
  {/if}

  {#if tapsAction}
    <div class="field">
      <span class="name">Ao tocar no visor</span>
      <ActionEditor bind:action={screen.action} id="screen" noneLabel="Nada (só mostra)" {edgePages} />
      {#if all.some((c) => c.type === 'timer')}
        <span class="muted small">No cronômetro, o toque continua controlando o tempo.</span>
      {/if}
    </div>
  {/if}

  <div class="field">
    <span class="name">Ao segurar o visor</span>
    <div class="holds" role="radiogroup" aria-label="Ao segurar o visor">
      {#each HOLD_KINDS as kind (kind.value)}
        <label class="hold">
          <input type="radio" name="screen-hold" checked={holdKind === kind.value} onchange={() => setHold(kind.value)} />
          <span>
            <span>{kind.label}</span>
            <span class="muted small">{kind.hint}</span>
          </span>
        </label>
      {/each}
    </div>

    {#if screen.hold?.type === 'action'}
      <ActionEditor bind:action={screen.hold.action} id="screen-hold" noneLabel="Escolha uma ação" {edgePages} />
    {:else if screen.hold?.type === 'cycle'}
      <ol class="sequence">
        <li class="step">
          <span class="num">1</span>
          <button type="button" class="step-name" aria-pressed={editing === null} onclick={() => (editing = null)}>
            {screenLabel(screen.content.type)}
          </button>
          <span class="muted small">principal</span>
        </li>
        {#each screen.hold.contents as content, i (i)}
          <li class="step">
            <span class="num">{i + 2}</span>
            <button type="button" class="step-name" aria-pressed={editing === i} onclick={() => (editing = editing === i ? null : i)}>
              {screenLabel(content.type)}
            </button>
            <button type="button" class="icon-btn" title="Subir" aria-label="Subir" disabled={i === 0} onclick={() => moveInCycle(i, -1)}>
              <Icon name="up" size={14} />
            </button>
            <button
              type="button"
              class="icon-btn"
              title="Descer"
              aria-label="Descer"
              disabled={i === screen.hold.contents.length - 1}
              onclick={() => moveInCycle(i, 1)}
            >
              <Icon name="down" size={14} />
            </button>
            <button type="button" class="icon-btn" title="Tirar da sequência" aria-label="Tirar da sequência" onclick={() => removeFromCycle(i)}>
              <Icon name="close" size={14} />
            </button>
          </li>
          {#if editing === i}
            <li class="step-options">{@render options(content, `hold-${i}`)}</li>
          {/if}
        {/each}
      </ol>
      {#if addable.length > 0}
        <select
          aria-label="Adicionar à sequência"
          value=""
          onchange={(e) => {
            addToCycle(e.currentTarget.value);
            e.currentTarget.value = '';
          }}
        >
          <option value="">Adicionar à sequência…</option>
          {#each addable as type (type.type)}<option value={type.type}>{type.label}</option>{/each}
        </select>
      {/if}
      <span class="muted small">
        {#if cycle.length === 0}
          Adicione o que o visor deve mostrar ao segurar.
        {:else}
          Depois do último, segurar volta ao principal. Toque num item para ver a prévia e as opções.
        {/if}
      </span>
    {/if}
  </div>

  {#if inRule}
    <button type="button" class="secondary" onclick={() => (screen = null)}>Voltar ao padrão nesta regra</button>
  {/if}
{/if}

<style>
  .head h2 {
    font-size: 18px;
    font-weight: 600;
  }
  .preview {
    aspect-ratio: 458 / 196;
    border-radius: 12px;
    overflow: hidden;
    background: var(--device);
    border: 1px solid var(--device-line);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    color: var(--device-text);
  }
  .preview img {
    width: 100%;
    height: 100%;
    display: block;
  }
  .previewing {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 10px;
  }
  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent-text);
    font-size: inherit;
    text-decoration: underline;
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
    gap: 8px;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
  }
  .types {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 6px;
  }
  .type {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 8px 10px;
    border-radius: 10px;
    border: 1px solid var(--line-soft);
    background: var(--surface);
    text-align: left;
  }
  .type:hover {
    background: var(--hover);
  }
  .type[aria-pressed='true'] {
    border: 2px solid var(--accent);
    padding: 7px 9px;
  }
  .type strong {
    font-size: 13px;
  }
  .type span {
    line-height: 1.3;
  }
  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .checks label,
  .minutes {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  input[type='checkbox'],
  input[type='radio'] {
    accent-color: var(--accent);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 16px;
  }
  .small-btn {
    padding: 5px 10px;
    font-size: 13px;
  }
  .small-btn[aria-pressed='true'] {
    border-color: var(--accent);
  }
  .minutes input {
    width: 72px;
  }
  textarea,
  select {
    width: 100%;
  }
  textarea {
    resize: vertical;
  }
  .holds {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hold {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 9px 12px;
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    background: var(--surface);
  }
  .hold:has(input:checked) {
    border-color: var(--accent);
  }
  .hold > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .hold input {
    margin-top: 3px;
  }
  .sequence {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .step {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .num {
    flex: 0 0 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 50%;
    background: var(--surface);
    border: 1px solid var(--line-soft);
    font-size: 12px;
  }
  .step-name {
    flex: 1 1 auto;
    min-width: 0;
    padding: 6px 10px;
    border-radius: 8px;
    border: 1px solid var(--line-soft);
    background: var(--surface);
    text-align: left;
  }
  .step-name:hover {
    background: var(--hover);
  }
  .step-name[aria-pressed='true'] {
    border-color: var(--accent);
  }
  .step-options {
    padding: 10px 12px;
    margin-left: 28px;
    border: 1px solid var(--line-soft);
    border-radius: 10px;
    background: var(--surface);
  }
  .icon-btn {
    flex: 0 0 30px;
    width: 30px;
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border-radius: 8px;
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
</style>
