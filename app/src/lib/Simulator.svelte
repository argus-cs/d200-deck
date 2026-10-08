<script lang="ts">
  import type { RuleStatus, Simulation } from './types';

  let {
    rules,
    simulation,
    onchange,
  }: { rules: RuleStatus[]; simulation: Simulation | null; onchange: (s: Simulation | null) => void } = $props();

  // Rules on the same app or site are the same choice here; keep the first.
  const targets = $derived.by(() => {
    const seen = new Set<string>();
    return rules.flatMap((rule, index) => {
      const id = `${rule.kind}:${rule.target}`;
      if (seen.has(id)) return [];
      seen.add(id);
      return [{ index, label: rule.kind === 'site' ? `Edge · ${rule.target}` : rule.target }];
    });
  });
  const current = $derived(simulation ?? { focus: null, open: [] });

  function setFocus(focus: number | null) {
    onchange({ ...current, focus });
  }

  function toggleOpen(index: number) {
    const open = current.open.includes(index) ? current.open.filter((i) => i !== index) : [...current.open, index];
    onchange({ ...current, open });
  }
</script>

<section class="card">
  <div>
    <h2>Simular contexto</h2>
    <p class="muted">Teste as regras sem abrir os apps.</p>
  </div>
  {#if targets.length === 0}
    <p class="muted">Nenhuma regra na config ainda.</p>
  {:else}
    <fieldset>
      <legend>Em foco</legend>
      <label>
        <input type="radio" name="sim-focus" checked={simulation !== null && current.focus === null} onchange={() => setFocus(null)} />
        Nenhum app com regra
      </label>
      {#each targets as target (target.index)}
        <label>
          <input type="radio" name="sim-focus" checked={current.focus === target.index} onchange={() => setFocus(target.index)} />
          <span class="mono">{target.label}</span>
        </label>
      {/each}
    </fieldset>
    <fieldset>
      <legend>Abertos</legend>
      {#each targets as target (target.index)}
        <label>
          <input
            type="checkbox"
            checked={current.open.includes(target.index) || current.focus === target.index}
            disabled={current.focus === target.index}
            onchange={() => toggleOpen(target.index)}
          />
          <span class="mono">{target.label}</span>
        </label>
      {/each}
    </fieldset>
  {/if}
  {#if simulation}
    <button type="button" class="secondary" onclick={() => onchange(null)}>Voltar ao contexto real</button>
  {:else}
    <p class="muted small">Desligado: o D200 segue o que está aberto de verdade.</p>
  {/if}
</section>

<style>
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  legend {
    padding: 0;
    margin-bottom: 6px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }
  label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 30px;
  }
  input {
    margin: 0;
    accent-color: var(--accent);
  }
</style>
