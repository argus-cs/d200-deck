<script lang="ts">
  import { onMount } from 'svelte';
  import { getUpdate, installUpdate, onUpdate, onUpdateProgress, openConfig, simulate, type Update } from './api';
  import Icon from './Icon.svelte';
  import type { Status } from './types';

  let { status, saveError }: { status: Status; saveError: string | null } = $props();

  /** A newer version found on GitHub, and how far its download is. */
  let update = $state<Update | null>(null);
  let downloaded = $state<number | null>(null);
  let updateError = $state<string | null>(null);

  /** Whether it is downloading comes from the app, so a download started from the tray shows here too. */
  function showUpdate(u: Update | null) {
    update = u;
    // A new download (maybe from the tray) clears the last failure, and one
    // that stopped (it failed) starts from zero the next time.
    if (u?.installing) updateError = null;
    else downloaded = null;
  }

  onMount(() => {
    getUpdate().then(showUpdate);
    const stopUpdate = onUpdate(showUpdate);
    const stopProgress = onUpdateProgress((percent) => (downloaded = percent));
    return () => {
      stopUpdate.then((unlisten) => unlisten());
      stopProgress.then((unlisten) => unlisten());
    };
  });

  /** On Windows this never returns: the installer closes the app and opens the new version. */
  async function startUpdate() {
    updateError = null;
    try {
      await installUpdate();
    } catch (e) {
      updateError = String(e);
    }
  }
</script>

{#if status.config_error}
  <div class="banner error" role="alert">
    <Icon name="alert" />
    <span><strong>O config.json tem um erro e não foi aplicado.</strong> {status.config_error}</span>
    <button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button>
  </div>
{/if}
{#if saveError}
  <div class="banner error" role="alert">
    <Icon name="alert" />
    <span><strong>Mudança ainda não salva:</strong> {saveError}</span>
  </div>
{/if}
{#if update}
  <div class="banner" role="status">
    <Icon name="refresh" />
    {#if update.installing}
      <span>Baixando a versão {update.version}{downloaded === null ? '' : ` (${downloaded}%)`}. O D200 Deck fecha e abre de novo sozinho.</span>
    {:else}
      <span>
        <strong>Versão {update.version} disponível.</strong>
        {updateError ? `A atualização falhou: ${updateError}` : 'O app fecha por alguns segundos e volta na versão nova.'}
      </span>
      <button type="button" class="primary" onclick={startUpdate}>{updateError ? 'Tentar de novo' : 'Atualizar e reiniciar'}</button>
    {/if}
  </div>
{/if}
{#if status.simulation}
  <div class="banner">
    <Icon name="live" />
    <span>Simulando contexto: o D200 mostra as regras do simulador, não o que está aberto.</span>
    <button type="button" class="secondary" onclick={() => simulate(null)}>Voltar ao real</button>
  </div>
{/if}

<style>
  .banner {
    flex: none;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 20px;
    background: var(--banner);
    border-bottom: 1px solid var(--banner-line);
  }
  .banner.error {
    background: var(--error);
    border-bottom-color: var(--error-line);
  }
</style>
