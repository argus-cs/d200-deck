<script lang="ts">
  import ActionEditor from './ActionEditor.svelte';
  import ColorPicker from './ColorPicker.svelte';
  import IconPicker from './IconPicker.svelte';
  import { screenUrl } from './icons.svelte';
  import { SCREEN_TYPES, newContent, screenLabel, type EdgePage, type Glyph, type Screen, type ScreenContent } from './types';

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

  const shown = $derived(screen ?? inherited);
  const preview = $derived(screenUrl(shown));
  const deviceDrawn = $derived(shown.content.type === 'device_clock' || shown.content.type === 'device_stats');

  function setType(type: ScreenContent['type']) {
    if (screen) screen.content = newContent(type);
  }

  /** Sets one option of the current content (24h, seconds, CPU…). */
  function setOption(name: string, value: boolean | number | string | null) {
    if (screen) (screen.content as Record<string, unknown>)[name] = value;
  }

  const option = (name: string) => (shown.content as Record<string, unknown>)[name];
</script>

<div class="head">
  <div class="muted small">Tecla 14</div>
  <h2>Visor</h2>
</div>

<div class="preview" class:device={deviceDrawn}>
  {#if deviceDrawn}
    <span>{screenLabel(shown.content.type)}</span>
    <span class="muted small">desenhado pelo próprio D200</span>
  {:else if preview}
    <img src={preview} alt="Prévia do visor" />
  {/if}
</div>

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

  {#if screen.content.type === 'clock' || screen.content.type === 'clock_stats'}
    <div class="checks">
      <label><input type="checkbox" checked={!!option('hour24')} onchange={(e) => setOption('hour24', e.currentTarget.checked)} /> 24 horas</label>
      <label><input type="checkbox" checked={!!option('seconds')} onchange={(e) => setOption('seconds', e.currentTarget.checked)} /> Segundos</label>
      {#if screen.content.type === 'clock'}
        <label><input type="checkbox" checked={!!option('date')} onchange={(e) => setOption('date', e.currentTarget.checked)} /> Data</label>
      {/if}
    </div>
  {:else if screen.content.type === 'stats'}
    <div class="checks">
      <label><input type="checkbox" checked={!!option('cpu')} onchange={(e) => setOption('cpu', e.currentTarget.checked)} /> CPU</label>
      <label><input type="checkbox" checked={!!option('memory')} onchange={(e) => setOption('memory', e.currentTarget.checked)} /> Memória</label>
      <label><input type="checkbox" checked={!!option('gpu')} onchange={(e) => setOption('gpu', e.currentTarget.checked)} /> GPU</label>
      <label><input type="checkbox" checked={!!option('network')} onchange={(e) => setOption('network', e.currentTarget.checked)} /> Rede</label>
    </div>
  {:else if screen.content.type === 'timer'}
    {@const minutes = Number(option('minutes') ?? 0)}
    <div class="field">
      <div class="checks">
        <label><input type="radio" name="timer-kind" checked={minutes === 0} onchange={() => setOption('minutes', 0)} /> Cronômetro (conta para cima)</label>
        <label><input type="radio" name="timer-kind" checked={minutes > 0} onchange={() => setOption('minutes', 25)} /> Contagem regressiva</label>
      </div>
      {#if minutes > 0}
        <div class="row">
          {#each [5, 15, 25, 50] as preset (preset)}
            <button type="button" class="secondary small-btn" aria-pressed={minutes === preset} onclick={() => setOption('minutes', preset)}>
              {preset === 25 ? '25 · Pomodoro' : preset} min
            </button>
          {/each}
          <label class="minutes">
            <input
              type="number"
              min="1"
              max="600"
              value={minutes}
              onchange={(e) => setOption('minutes', Math.max(1, Math.min(600, Number(e.currentTarget.value) || 1)))}
            /> min
          </label>
        </div>
      {/if}
      <span class="muted small">No aparelho: toque inicia e pausa; segurar o visor zera.</span>
    </div>
  {:else if screen.content.type === 'image'}
    <div class="field">
      <span class="name">Imagem</span>
      <IconPicker icon={option('icon') as string | null} color={screen.background} {glyphs} {edgePages} onchange={(icon) => setOption('icon', icon)} />
    </div>
  {:else if screen.content.type === 'text'}
    <div class="field">
      <label class="name" for="screen-text">Texto (até 3 linhas)</label>
      <textarea id="screen-text" rows="3" value={String(option('text') ?? '')} oninput={(e) => setOption('text', e.currentTarget.value)}
      ></textarea>
    </div>
  {:else if screen.content.type === 'now_playing'}
    <p class="muted small">Mostra o que estiver tocando no Windows: Spotify, YouTube no Edge, qualquer player que aparece no controle de mídia.</p>
  {/if}

  {#if !deviceDrawn}
    <div class="field">
      <span class="name">Cores</span>
      <ColorPicker label="Fundo" value={screen.background} onchange={(c) => screen && c && (screen.background = c)} />
      <ColorPicker label="Texto" value={screen.color} onchange={(c) => screen && c && (screen.color = c)} />
      <ColorPicker label="Destaque (barras, avisos)" value={screen.accent} onchange={(c) => screen && c && (screen.accent = c)} />
    </div>
  {/if}

  {#if screen.content.type !== 'timer'}
    <ActionEditor bind:action={screen.action} id="screen" noneLabel="Nada (só mostra)" {edgePages} />
    <p class="muted small">É o que acontece ao tocar no visor.</p>
  {/if}

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
  textarea {
    width: 100%;
    resize: vertical;
  }
</style>
