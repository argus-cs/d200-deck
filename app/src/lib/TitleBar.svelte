<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { onMount } from 'svelte';
  import { resend, setPaused } from './api';
  import Icon from './Icon.svelte';
  import type { Status } from './types';

  let {
    status,
    saveState,
    canUndo,
    canRedo,
    onundo,
    onredo,
  }: {
    status: Status;
    saveState: 'idle' | 'saving' | 'saved';
    canUndo: boolean;
    canRedo: boolean;
    onundo: () => void;
    onredo: () => void;
  } = $props();

  const appWindow = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    appWindow.isMaximized().then((m) => (maximized = m));
    const stopResize = appWindow.onResized(async () => (maximized = await appWindow.isMaximized()));
    return () => {
      stopResize.then((unlisten) => unlisten());
    };
  });
</script>

<!-- The window has no frame: this is its title bar, dragged by the empty parts. -->
<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region><Icon name="grid" size={20} /> D200 Deck</div>
  <div class="status" data-tauri-drag-region>
    <span class="pill" class:off={!status.device} title={status.device_error ?? undefined} data-tauri-drag-region>
      <span class="led"></span>{status.device ? 'D200 conectado' : status.device_error ? 'Sem acesso ao USB' : 'D200 desconectado'}
    </span>
    <span class="pill" class:off={!status.extension} data-tauri-drag-region>
      <span class="led"></span>{status.extension ? 'Extensão conectada' : 'Extensão desconectada'}
    </span>
  </div>
  <div class="drag" data-tauri-drag-region></div>
  <div class="tools">
    <span class="save-state" aria-live="polite">
      {#if saveState === 'saving'}<span class="muted">Salvando…</span>
      {:else if saveState === 'saved'}<span class="saved"><Icon name="check" size={14} />Salvo</span>{/if}
    </span>
    <button type="button" class="tool icon" title="Desfazer (Ctrl+Z)" aria-label="Desfazer" disabled={!canUndo} onclick={onundo}>
      <Icon name="undo" size={16} />
    </button>
    <button type="button" class="tool icon" title="Refazer (Ctrl+Y)" aria-label="Refazer" disabled={!canRedo} onclick={onredo}>
      <Icon name="redo" size={16} />
    </button>
    <button type="button" class="tool" aria-pressed={status.paused} onclick={() => setPaused(!status.paused)}>
      <Icon name={status.paused ? 'play' : 'pause'} size={16} />{status.paused ? 'Retomar regras' : 'Pausar regras'}
    </button>
    <button type="button" class="tool icon" title="Reenviar tudo para o D200" aria-label="Reenviar tudo para o D200" onclick={() => resend()}>
      <Icon name="refresh" size={16} />
    </button>
  </div>
  <div class="window-buttons">
    <button type="button" class="win" aria-label="Minimizar" title="Minimizar" onclick={() => appWindow.minimize()}>
      <Icon name="minimize" size={14} />
    </button>
    <button
      type="button"
      class="win"
      aria-label={maximized ? 'Restaurar' : 'Maximizar'}
      title={maximized ? 'Restaurar' : 'Maximizar'}
      onclick={() => appWindow.toggleMaximize()}
    >
      <Icon name={maximized ? 'restore' : 'maximize'} size={13} />
    </button>
    <button type="button" class="win close" aria-label="Fechar" title="Fechar (o D200 Deck continua na bandeja)" onclick={() => appWindow.close()}>
      <Icon name="close" size={14} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    flex: none;
    height: 44px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding-left: 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line-soft);
    user-select: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 600;
    font-size: 14px;
    white-space: nowrap;
  }
  .brand :global(.ico) {
    color: var(--accent);
    pointer-events: none;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-left: 8px;
  }
  .drag {
    flex: 1 1 auto;
    align-self: stretch;
  }
  .pill {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--soft);
    white-space: nowrap;
  }
  .led {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
    pointer-events: none;
  }
  .pill.off .led {
    background: transparent;
    border: 1.5px solid var(--muted);
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .save-state {
    min-width: 62px;
    font-size: 12px;
    text-align: right;
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--accent-text);
  }
  .tool {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--line);
    background: transparent;
    font-size: 13px;
    white-space: nowrap;
  }
  .tool.icon {
    width: 30px;
    padding: 0;
    justify-content: center;
  }
  .tool:hover:not(:disabled) {
    background: var(--hover);
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .tool[aria-pressed='true'] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--ink);
  }
  .window-buttons {
    display: flex;
    align-self: stretch;
    margin-left: 6px;
  }
  .win {
    width: 46px;
    border: 0;
    border-radius: 0;
    background: transparent;
    color: var(--soft);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .win:hover {
    background: var(--hover);
    color: var(--text);
  }
  .win.close:hover {
    background: var(--close-hover);
    color: #fff;
  }
</style>
