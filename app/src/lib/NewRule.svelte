<script lang="ts">
  import { onMount } from 'svelte';
  import { runningApps } from './api';
  import ExtensionGuide from './ExtensionGuide.svelte';
  import Icon from './Icon.svelte';
  import type { Mode, Rule } from './types';

  let {
    edgeTabs,
    extension,
    oncreate,
    oncancel,
  }: { edgeTabs: string[]; extension: boolean; oncreate: (rule: Rule) => void; oncancel: () => void } = $props();

  let kind = $state<'app' | 'site'>('app');
  let apps = $state<{ exe: string; title: string }[]>([]);
  let query = $state('');
  let exe = $state('');
  let site = $state('');
  let mode = $state<Mode>('focus');
  let name = $state('');

  async function loadApps() {
    const found = await runningApps();
    apps = found.filter((a) => a.exe.toLowerCase() !== 'd200-deck.exe');
  }
  onMount(loadApps);

  const shown = $derived(
    apps.filter((a) => !query || `${a.exe} ${a.title}`.toLowerCase().includes(query.trim().toLowerCase())),
  );
  const host = $derived(site.trim().toLowerCase().replace(/^https?:\/\//, '').replace(/^www\./, '').replace(/\/+$/, ''));
  const target = $derived(kind === 'app' ? exe.trim() : host);
  const autoName = $derived(kind === 'app' ? exe.replace(/\.exe$/i, '') : host);
  const openText = $derived(
    kind === 'app'
      ? `Enquanto ${target || 'o app'} estiver rodando, mesmo minimizado ou atrás de outras janelas.`
      : `Enquanto houver uma aba de ${target || 'o site'} aberta no Edge, mesmo em segundo plano.`,
  );
  const focusText = $derived(
    kind === 'app'
      ? `Só com a janela de ${target || 'o app'} na frente. Deu Alt+Tab, as teclas voltam.`
      : `Só com o Edge na frente e a aba de ${target || 'o site'} selecionada.`,
  );

  function create() {
    if (!target) return;
    oncreate({
      name: name.trim() || autoName,
      when: kind === 'app' ? { process: target } : { site: target },
      mode,
      enabled: true,
      keys: {},
    });
  }
</script>

<div class="page scroll">
<div class="content">
  <div>
    <h1>Nova regra</h1>
    <p class="muted">Escolha quando a regra vale. As teclas você define no passo seguinte, direto no D200 virtual.</p>
  </div>

  <section>
    <div class="section-head">
      <h2>1. Quando ativar</h2>
      <div class="segmented" role="group" aria-label="Tipo de regra">
        <button type="button" aria-pressed={kind === 'app'} onclick={() => (kind = 'app')}><Icon name="app" size={16} />App</button>
        <button type="button" aria-pressed={kind === 'site'} onclick={() => (kind = 'site')}><Icon name="site" size={16} />Site no Edge</button>
      </div>
    </div>

    {#if kind === 'app'}
      <div class="box">
        <div class="row">
          <label class="sr" for="app-search">Buscar app aberto</label>
          <input id="app-search" class="grow" type="text" bind:value={query} placeholder="Buscar entre os apps abertos agora" />
          <button type="button" class="secondary" onclick={loadApps}><Icon name="refresh" size={16} /> Atualizar</button>
        </div>
        <div class="apps">
          {#each shown as app (app.exe)}
            <button type="button" class="app" aria-pressed={exe === app.exe} onclick={() => (exe = app.exe)}>
              <span class="initial" aria-hidden="true">{app.exe.charAt(0).toUpperCase()}</span>
              <span class="app-text">
                <span class="ellipsis">{app.title}</span>
                <span class="mono muted">{app.exe}</span>
              </span>
            </button>
          {/each}
        </div>
        {#if shown.length === 0}<p class="muted">Nenhum app aberto com esse nome.</p>{/if}
        <div class="row">
          <label for="app-exe" class="muted">Ou digite o nome do .exe</label>
          <input id="app-exe" class="mono grow" type="text" bind:value={exe} placeholder="Discord.exe" />
        </div>
      </div>
    {:else}
      {#if !extension}<ExtensionGuide />{/if}
      <div class="box">
        <label class="name" for="site">Endereço</label>
        <input id="site" class="mono" type="text" bind:value={site} placeholder="youtube.com ou github.com/*/pulls" />
        <p class="muted small">
          Um domínio vale para ele e para os subdomínios (www., m.…). Use * como curinga no caminho, como em github.com/*/pulls.
        </p>
        {#if edgeTabs.length > 0}
          <span class="label-caps">Abas abertas agora no Edge</span>
          <div class="row">
            {#each edgeTabs as tab (tab)}
              <button type="button" class="chip" aria-pressed={host === tab} onclick={() => (site = tab)}>{tab}</button>
            {/each}
          </div>
        {:else}
          <p class="muted small">Com a extensão do Edge conectada, as abas abertas aparecem aqui.</p>
        {/if}
      </div>
    {/if}
  </section>

  <section>
    <h2>2. Modo</h2>
    <div class="modes" role="group" aria-label="Modo da regra">
      <button type="button" class="mode" aria-pressed={mode === 'open'} onclick={() => (mode = 'open')}>
        <strong>Aberto</strong><span>{openText}</span>
      </button>
      <button type="button" class="mode" aria-pressed={mode === 'focus'} onclick={() => (mode = 'focus')}>
        <strong>Foco</strong><span>{focusText}</span>
      </button>
    </div>
    <p class="muted small">Se duas regras quiserem a mesma tecla: foco ganha de aberto; no mesmo modo, a regra mais abaixo na lista ganha.</p>
  </section>

  <section>
    <h2>3. Nome</h2>
    <label class="sr" for="rule-name">Nome da regra</label>
    <input id="rule-name" class="narrow" type="text" bind:value={name} placeholder={autoName || 'Nome da regra'} />
  </section>

  <div class="footer">
    <button type="button" class="secondary" onclick={oncancel}>Cancelar</button>
    <button type="button" class="primary" disabled={!target} onclick={create}>Criar e escolher as teclas</button>
  </div>
</div>
</div>

<style>
  .page {
    flex: 1 1 auto;
    min-width: 0;
  }
  .content {
    max-width: 900px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 26px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
  }
  h2 {
    font-size: 16px;
    font-weight: 600;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .section-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .segmented {
    display: flex;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--line);
    border-radius: 11px;
  }
  .segmented button {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 0;
    border-radius: 8px;
    padding: 7px 14px;
    background: transparent;
    color: var(--soft);
    font-weight: 500;
  }
  .segmented button[aria-pressed='true'] {
    background: var(--accent);
    color: var(--ink);
  }
  .box {
    background: var(--raised);
    border: 1px solid var(--line-soft);
    border-radius: 16px;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .grow {
    flex: 1 1 200px;
    min-width: 0;
  }
  input[type='text'] {
    border-radius: 10px;
    padding: 9px 12px;
  }
  .narrow {
    max-width: 420px;
  }
  .apps {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 8px;
  }
  .app {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid var(--line-soft);
    background: var(--surface);
    text-align: left;
    min-width: 0;
  }
  .app[aria-pressed='true'] {
    border: 2px solid var(--accent);
    background: var(--selected);
  }
  .initial {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 8px;
    background: var(--key);
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 600;
  }
  .app-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .name {
    font-size: 13px;
    font-weight: 500;
  }
  .label-caps {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .chip {
    border-radius: 999px;
    padding: 6px 12px;
    border: 1px solid var(--line);
    background: var(--surface);
    font-family: var(--mono);
    font-size: 13px;
  }
  .chip[aria-pressed='true'] {
    border: 2px solid var(--accent);
  }
  .modes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 12px;
  }
  .mode {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 16px 18px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--raised);
    text-align: left;
  }
  .mode strong {
    font-size: 15px;
  }
  .mode span {
    color: var(--soft);
  }
  .mode[aria-pressed='true'] {
    border: 2px solid var(--accent);
    background: var(--surface);
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
</style>
