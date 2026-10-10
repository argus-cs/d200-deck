<script lang="ts">
  import Icon from './Icon.svelte';

  let {
    value = $bindable(''),
    label,
    placeholder = '',
    grow = false,
    oninput,
    onkeydown,
  }: {
    value?: string;
    /** What is searched, for screen readers. */
    label: string;
    placeholder?: string;
    /** Takes the room left in a row. */
    grow?: boolean;
    oninput?: (event: Event) => void;
    onkeydown?: (event: KeyboardEvent) => void;
  } = $props();
</script>

<div class="search" class:grow>
  <Icon name="search" size={16} />
  <input type="text" aria-label={label} {placeholder} bind:value {oninput} {onkeydown} />
</div>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted);
  }
  .search input {
    flex: 1 1 auto;
    min-width: 0;
    border: 0;
    padding: 8px 0;
    background: transparent;
  }
  .search input:focus-visible {
    outline: none;
  }
  .search:focus-within {
    outline: 2px solid var(--text);
    outline-offset: 2px;
  }
  .grow {
    flex: 1 1 auto;
  }
</style>
