<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { openAddonsFolder, openAddonsGuide, openLog, rescanAddons, retryAddon } from './api';
  import Icon from './Icon.svelte';
  import { store, storedJson } from './storage';
  import type { AddonState, AddonStatus, Config } from './types';

  let { config = $bindable(), list, configPath }: { config: Config; list: AddonStatus[]; configPath: string } = $props();

  const TRUSTED = 'trustedAddons';
  const STATE: Record<AddonState, string> = {
    off: 'Desligado',
    running: 'Rodando',
    restarting: 'Abrindo de novo',
    stopped: 'Parou',
    invalid: 'Com erro',
  };

  /** Addons the person already agreed to run: switching them on asks nothing. */
  let trusted = $state<string[]>(storedJson<string[]>(TRUSTED, []));
  /** The folder of the addon waiting for a yes. */
  let confirming = $state<string | null>(null);
  let failure = $state<string | null>(null);

  const folder = $derived(configPath.replace(/[^\\/]*$/, 'addons'));
  /** On as the editors have it (the status follows once the engine applies the config). */
  const isOn = (addon: AddonStatus) => (config.addons ?? []).includes(addon.id);

  // Addons copied into the folder show up when the page opens.
  onMount(() => {
    run(rescanAddons);
  });

  async function run(work: () => Promise<unknown>) {
    failure = null;
    try {
      await work();
    } catch (e) {
      failure = String(e);
    }
  }

  function toggle(addon: AddonStatus) {
    if (isOn(addon)) {
      enable(addon.id, false);
    } else if (trusted.includes(addon.id)) {
      enable(addon.id, true);
    } else {
      confirming = addon.folder;
      // It opens below the addon, maybe out of sight.
      tick().then(() => document.querySelector('.confirm')?.scrollIntoView({ block: 'nearest', behavior: 'smooth' }));
    }
  }

  function enable(id: string, on: boolean) {
    const ids = (config.addons ?? []).filter((other) => other !== id);
    if (on) ids.push(id);
    // None on is left out of config.json.
    if (ids.length > 0) config.addons = ids;
    else delete config.addons;
  }

  function trust(addon: AddonStatus) {
    trusted = [...trusted, addon.id];
    store(TRUSTED, JSON.stringify(trusted));
    confirming = null;
    enable(addon.id, true);
  }
</script>

<div class="page scroll">
  <div class="content">
    <div class="intro">
      <h1>Addons</h1>
      <p class="soft">
        Addons trazem ações novas para as teclas: jogos, apps e serviços que o D200 Deck não conhece. Cada addon é um programa que roda junto
        com o app e pode mudar o que a tecla mostra.
      </p>
    </div>

    <div class="toolbar">
      <button type="button" class="secondary with-icon" onclick={() => run(openAddonsFolder)}><Icon name="folder" size={16} />Abrir a pasta de addons</button>
      <button type="button" class="secondary with-icon" onclick={() => run(rescanAddons)}><Icon name="refresh" size={16} />Procurar de novo</button>
      <button type="button" class="secondary with-icon" onclick={() => run(openAddonsGuide)}><Icon name="file" size={16} />Como fazer um addon</button>
      <span class="mono muted small path ellipsis" title={folder}>{folder}</span>
    </div>
    {#if failure}<p class="error-text small">{failure}</p>{/if}

    {#if list.length === 0}
      <div class="notice">
        <p>Nenhum addon na pasta ainda.</p>
        <p class="muted small">
          Cada addon é uma pasta com um <span class="mono">addon.json</span> dentro da pasta de addons. O guia "Como fazer um addon" explica o formato
          e traz um exemplo pronto para copiar.
        </p>
      </div>
    {/if}

    {#each list as addon (addon.folder)}
      {@const invalid = addon.state === 'invalid'}
      {@const on = !invalid && isOn(addon)}
      {@const isNew = !invalid && !on && !trusted.includes(addon.id)}
      <section class="addon" class:invalid class:asking={confirming === addon.folder}>
        <div class="head">
          <div class="tile" class:bad={invalid}><Icon name={invalid ? 'alert' : 'puzzle'} size={24} /></div>
          <div class="info">
            <div class="title">
              <h2 class:mono={invalid}>{invalid ? addon.folder : addon.name}</h2>
              {#if !invalid}
                <span class="mono muted small">{addon.version}</span>
                {#if addon.author}<span class="muted small">{addon.author}</span>{/if}
              {/if}
              {#if isNew}<span class="badge">Novo</span>{/if}
            </div>
            {#if invalid}
              <p class="error-text small">Não foi carregado: {addon.error}</p>
            {:else}
              {#if addon.description}<p class="soft">{addon.description}</p>{/if}
              {#if addon.actions.length > 0}
                <div class="chips">{#each addon.actions as action (action.id)}<span class="chip">{action.name}</span>{/each}</div>
              {/if}
              {#if isNew}<p class="muted small">Chegou desligado: addons novos só rodam depois que você liga.</p>{/if}
            {/if}
          </div>
          {#if !invalid}
            <div class="side">
              <span class="state {on ? addon.state : 'off'}"><span class="dot"></span>{STATE[on ? addon.state : 'off']}</span>
              <button
                type="button"
                class="switch"
                role="switch"
                aria-checked={on}
                aria-label={`Ligar o ${addon.name}`}
                onclick={() => toggle(addon)}
              >
                <span></span>
              </button>
            </div>
          {/if}
        </div>

        {#if on && addon.error}
          <div class="problem small">
            <p>{addon.error}</p>
            <div class="buttons">
              {#if addon.state === 'stopped'}
                <button type="button" class="secondary small-btn" onclick={() => run(() => retryAddon(addon.id))}>Tentar de novo</button>
              {/if}
              <button type="button" class="secondary small-btn" onclick={() => run(openLog)}>Abrir o log</button>
            </div>
          </div>
        {/if}

        {#if confirming === addon.folder}
          <div class="confirm">
            <h3><Icon name="alert" size={18} />Ligar o {addon.name}?</h3>
            <p>
              Um addon é um programa. Ligado, ele roda junto com o D200 Deck e tem as mesmas permissões que você no Windows: pode ler e mudar os
              seus arquivos, acessar a internet e apertar teclas. Ligue só addons de quem você confia.
            </p>
            <dl>
              <dt>Autor</dt>
              <dd>{addon.author || 'não informado'}</dd>
              <dt>Vai rodar</dt>
              <dd class="mono">{addon.run}</dd>
              <dt>Na pasta</dt>
              <dd class="mono">{addon.path}</dd>
              {#if addon.actions.length > 0}
                <dt>Ações</dt>
                <dd>{addon.actions.map((action) => action.name).join(', ')}</dd>
              {/if}
            </dl>
            <div class="buttons">
              <button type="button" class="primary" onclick={() => trust(addon)}>Ligar o addon</button>
              <button type="button" class="secondary" onclick={() => (confirming = null)}>Agora não</button>
              <span class="muted small">Só pergunta na primeira vez. Depois, a chave liga e desliga direto.</span>
            </div>
          </div>
        {/if}
      </section>
    {/each}

    {#if list.length > 0}
      <p class="muted small">
        Desligar ou remover um addon não apaga as teclas dele: elas escurecem e voltam a funcionar quando ele voltar. Atalhos e ações dos addons
        rodam com as suas permissões do Windows.
      </p>
    {/if}
  </div>
</div>

<style>
  .page {
    flex: 1 1 auto;
    min-width: 0;
  }
  .content {
    max-width: 920px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .intro {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
  }
  .toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .with-icon {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .path {
    margin-left: auto;
    max-width: 320px;
  }
  .addon {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 16px 18px;
    border: 1px solid var(--line-soft);
    border-radius: 16px;
    background: var(--raised);
  }
  .addon.invalid {
    background: transparent;
    border-style: dashed;
    border-color: var(--line-strong);
  }
  .addon.asking {
    border-color: var(--banner-line);
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 16px;
  }
  .tile {
    flex: none;
    width: 48px;
    height: 48px;
    border-radius: 12px;
    background: var(--key);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .tile.bad {
    background: var(--error);
    color: var(--error-text);
  }
  .info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 10px;
  }
  h2 {
    font-size: 16px;
    font-weight: 600;
  }
  .badge {
    padding: 0 8px;
    border-radius: 999px;
    border: 1px solid var(--accent);
    color: var(--accent-text);
    font-size: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    padding: 2px 10px;
    border-radius: 999px;
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--soft);
  }
  .side {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding-top: 2px;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    font-size: 13px;
    color: var(--muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    border: 1.5px solid var(--muted);
  }
  .state.running {
    color: var(--soft);
  }
  .state.running .dot {
    border-color: var(--accent);
    background: var(--accent);
  }
  .state.stopped,
  .state.restarting {
    color: var(--error-text);
  }
  .state.stopped .dot,
  .state.restarting .dot {
    border-color: var(--error-text);
    background: var(--error-text);
  }
  .switch {
    position: relative;
    width: 44px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 999px;
    background: var(--line-strong);
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--muted);
    transition: left 0.15s;
  }
  .switch[aria-checked='true'] {
    background: var(--accent);
  }
  .switch[aria-checked='true'] span {
    left: 23px;
    background: var(--ink);
  }
  .problem {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--error);
    border: 1px solid var(--error-line);
    color: var(--error-text);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .problem .secondary {
    color: var(--text);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border-radius: 12px;
    background: var(--banner);
    border: 1px solid var(--banner-line);
  }
  .confirm h3 {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 16px;
    font-weight: 600;
  }
  .confirm h3 :global(svg) {
    color: var(--accent-text);
  }
  dl {
    display: grid;
    grid-template-columns: 110px minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--muted);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
