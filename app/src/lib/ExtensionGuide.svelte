<script lang="ts">
  import { onMount } from 'svelte';
  import { extensionFolder, openEdgeExtensions, openExtensionFolder } from './api';
  import Icon from './Icon.svelte';

  let { ondismiss }: { ondismiss?: () => void } = $props();

  let folder = $state('');
  let error = $state<string | null>(null);

  onMount(() => {
    extensionFolder()
      .then((path) => (folder = path))
      .catch((e) => (error = String(e)));
  });

  async function run(task: () => Promise<unknown>) {
    error = null;
    try {
      await task();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<section class="card guide">
  <div class="head">
    <Icon name="puzzle" size={22} />
    <div class="grow">
      <h2>Ative a extensão do Edge</h2>
      <p class="muted small">As regras de site precisam dela para saber quais abas estão abertas. É uma vez só.</p>
    </div>
    {#if ondismiss}<button type="button" class="secondary small-btn" onclick={ondismiss}>Agora não</button>{/if}
  </div>
  <ol>
    <li>
      <span>Abra a página de extensões do Edge.</span>
      <button type="button" class="secondary small-btn" onclick={() => run(openEdgeExtensions)}>Abrir extensões do Edge</button>
    </li>
    <li><span>Ligue <strong>Modo de desenvolvedor</strong> (chave no menu da esquerda).</span></li>
    <li>
      <span>Clique em <strong>Carregar sem pacote</strong> e escolha esta pasta:</span>
      <code class="mono">{folder || '…'}</code>
      <button type="button" class="secondary small-btn" onclick={() => run(openExtensionFolder)}>Mostrar a pasta</button>
    </li>
    <li><span>Pronto: em até 30 segundos a extensão conecta e este aviso some.</span></li>
  </ol>
  {#if error}<p class="error-text small">{error}</p>{/if}
</section>

<style>
  .guide {
    flex: none;
    border-color: var(--banner-line);
    background: var(--banner);
  }
  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
  }
  .head :global(.ico) {
    color: var(--accent-text);
    margin-top: 2px;
  }
  .grow {
    flex: 1 1 auto;
  }
  ol {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  li span {
    margin-right: 8px;
  }
  code {
    display: inline-block;
    margin: 4px 8px 4px 0;
    padding: 3px 8px;
    border-radius: 6px;
    background: var(--surface);
    border: 1px solid var(--line-soft);
    user-select: all;
    word-break: break-all;
  }
  .small-btn {
    padding: 5px 10px;
    font-size: 13px;
  }
</style>
