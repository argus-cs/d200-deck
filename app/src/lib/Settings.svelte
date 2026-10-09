<script lang="ts">
  import { getVersion } from '@tauri-apps/api/app';
  import { checkUpdate, openConfig } from './api';
  import ColorPicker from './ColorPicker.svelte';
  import type { Config } from './types';

  let { config = $bindable(), configPath }: { config: Config; configPath: string } = $props();

  // config.json keeps the label color as hex without "#".
  const labelColor = $derived(`#${config.label.color}`);

  let version = $state('');
  let checking = $state(false);
  let answer = $state<string | null>(null);
  getVersion().then((v) => (version = v));

  /** A version found shows at the top of the window, with its button. */
  async function lookForUpdate() {
    checking = true;
    answer = null;
    try {
      const found = await checkUpdate();
      answer = found ? `A versão ${found.version} está disponível: use o aviso no topo da janela.` : 'Você já tem a versão mais nova.';
    } catch (e) {
      answer = `Não deu para procurar agora: ${e}`;
    } finally {
      checking = false;
    }
  }
</script>

<div class="page scroll">
<div class="content">
  <h1>Ajustes</h1>

  <section class="card">
    <h2>Aparelho</h2>
    <div class="field">
      <label for="brightness">Brilho <span class="mono muted">{config.brightness}%</span></label>
      <input id="brightness" type="range" min="0" max="100" bind:value={config.brightness} />
    </div>
    <p class="muted small">O visor (a tela larga ao lado das teclas) é editado clicando nele no Layout padrão ou numa regra.</p>
  </section>

  <section class="card">
    <h2>Texto nas teclas</h2>
    <label class="check"><input type="checkbox" bind:checked={config.label.show} /> Mostrar o texto embaixo do ícone</label>
    <div class="field">
      <label for="label-size">Tamanho</label>
      <input id="label-size" type="number" min="6" max="20" bind:value={config.label.size} />
    </div>
    <div class="field">
      <span class="label">Cor padrão</span>
      <ColorPicker label="Texto" value={labelColor} onchange={(c) => c && (config.label.color = c.slice(1))} />
      <span class="muted small">Cada tecla pode ter a sua no editor; esta vale para as que não têm.</span>
    </div>
  </section>

  <section class="card">
    <h2>Arquivo de configuração</h2>
    <p class="mono muted small">{configPath}</p>
    <p class="muted small">Tudo o que você muda aqui é gravado nesse arquivo. Ele também pode ser editado à mão; o app percebe e aplica.</p>
    <div><button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button></div>
  </section>

  <section class="card">
    <h2>Atualizações</h2>
    <p>Versão instalada: <span class="mono">{version || '…'}</span></p>
    <p class="muted small">
      O app procura uma versão nova ao abrir e a cada 6 horas, e só atualiza quando você manda (aqui, no aviso do topo ou no menu da bandeja).
    </p>
    <div class="row">
      <button type="button" class="secondary" disabled={checking} onclick={lookForUpdate}>
        {checking ? 'Procurando…' : 'Procurar atualizações'}
      </button>
      {#if answer}<span class="small">{answer}</span>{/if}
    </div>
  </section>

  <p class="muted small">Para iniciar com o Windows, use o menu do ícone na bandeja. O tema claro ou escuro segue o do Windows.</p>
</div>
</div>

<style>
  .page {
    flex: 1 1 auto;
    min-width: 0;
  }
  .content {
    max-width: 720px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
  }
  .card {
    flex: none;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label,
  .label {
    font-weight: 500;
  }
  input[type='number'] {
    max-width: 320px;
  }
  input[type='range'] {
    max-width: 320px;
    accent-color: var(--accent);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 400;
  }
  .check input {
    accent-color: var(--accent);
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
  }
</style>
