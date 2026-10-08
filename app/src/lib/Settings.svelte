<script lang="ts">
  import { openConfig } from './api';
  import type { Config } from './types';

  let { config = $bindable(), configPath }: { config: Config; configPath: string } = $props();

  // The device takes the label color as hex without "#".
  const labelColor = $derived(`#${config.label.color}`);
</script>

<div class="page">
  <h1>Ajustes</h1>

  <section class="card">
    <h2>Aparelho</h2>
    <div class="field">
      <label for="brightness">Brilho <span class="mono muted">{config.brightness}%</span></label>
      <input id="brightness" type="range" min="0" max="100" bind:value={config.brightness} />
    </div>
    <div class="field">
      <label for="window">Visor (a tela larga ao lado das teclas)</label>
      <select id="window" bind:value={config.window}>
        <option value="clock">Relógio</option>
        <option value="stats">Uso do PC (CPU e memória)</option>
        <option value="image">Imagem da tecla 14</option>
      </select>
    </div>
  </section>

  <section class="card">
    <h2>Texto nas teclas</h2>
    <label class="check"><input type="checkbox" bind:checked={config.label.show} /> Mostrar o texto embaixo do ícone</label>
    <div class="row">
      <div class="field">
        <label for="label-size">Tamanho</label>
        <input id="label-size" type="number" min="6" max="24" bind:value={config.label.size} />
      </div>
      <div class="field">
        <label for="label-color">Cor</label>
        <input
          id="label-color"
          type="color"
          value={labelColor}
          oninput={(e) => (config.label.color = e.currentTarget.value.slice(1).toUpperCase())}
        />
      </div>
    </div>
  </section>

  <section class="card">
    <h2>Arquivo de configuração</h2>
    <p class="mono muted small">{configPath}</p>
    <p class="muted small">Tudo o que você muda aqui é gravado nesse arquivo. Ele também pode ser editado à mão; o app percebe e aplica.</p>
    <div><button type="button" class="secondary" onclick={() => openConfig()}>Abrir config.json</button></div>
  </section>

  <p class="muted small">Para iniciar com o Windows, use o menu do ícone na bandeja.</p>
</div>

<style>
  .page {
    flex: 999 1 640px;
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
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 20px;
  }
  label {
    font-weight: 500;
  }
  select,
  input[type='number'] {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 8px 10px;
    color: var(--text);
    font: inherit;
    max-width: 320px;
  }
  input[type='range'] {
    max-width: 320px;
    accent-color: var(--accent);
  }
  input[type='color'] {
    width: 44px;
    height: 34px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: transparent;
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
</style>
