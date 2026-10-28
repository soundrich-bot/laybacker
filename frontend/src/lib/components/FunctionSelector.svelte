<script>
  import { INTENT_GROUPS, HERO_INTENT } from '../intents.js';
  import { sp } from '../spell.svelte.js';
  // Every function as a key on the empty screen, in a grid grouped by what
  // the job does. Titles only; what each does is a hover tip. Optional: a
  // key only changes what the drop zone asks for and where the drop lands.
  let { intent = null, onSelect } = $props();
</script>

<div class="selector" role="group" aria-label="Pick a job">
  <span class="lead">OR PICK A JOB</span>
  <button class="tile hero" class:active={intent === HERO_INTENT.key}
    onclick={() => onSelect(intent === HERO_INTENT.key ? null : HERO_INTENT.key)}
    title="{HERO_INTENT.desc}. {HERO_INTENT.hint}">{HERO_INTENT.title}</button>
  <div class="grid">
    {#each INTENT_GROUPS as group (group.label)}
      <div class="col">
        <span class="col-label">{group.label}</span>
        {#each group.intents as it (it.key)}
          <button class="tile" class:active={intent === it.key} onclick={() => onSelect(intent === it.key ? null : it.key)}
            title="{it.desc}. {it.hint}">{sp(it.title)}</button>
        {/each}
      </div>
    {/each}
  </div>
</div>

<style>
  .selector { margin: 0 var(--gap-lg) var(--gap-md); display: flex; flex-direction: column; align-items: center; gap: 8px; flex-shrink: 0; }
  .lead { font-family: var(--font-display); font-weight: 700; font-size: 11px; letter-spacing: 0.04em; color: var(--text-muted); }
  .grid { display: grid; grid-template-columns: repeat(4, 168px); gap: 0 18px; justify-content: center; }
  .col { display: flex; flex-direction: column; gap: 6px; }
  .col-label { font-family: var(--font-display); font-weight: 700; font-size: 10px; letter-spacing: 0.04em; color: var(--text-muted); text-align: center; padding-bottom: 2px; border-bottom: 1px solid var(--border-color); margin-bottom: 2px; }
  .tile {
    font-family: var(--font-display); font-weight: 700; font-size: 11px; letter-spacing: 0.04em; text-align: center;
    color: var(--text-secondary); background: var(--cap-face); border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    padding: 7px 10px; cursor: pointer; box-shadow: var(--cap-shadow); transition: all 0.15s; white-space: nowrap;
  }
  .tile:hover { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); box-shadow: var(--cap-shadow-hover); }
  .tile:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .tile.active { color: var(--bg-dark); background: var(--neon-cyan); border-color: var(--neon-cyan); }
  .tile.hero { min-width: 354px; padding: 9px 16px; font-size: 12.5px; color: var(--text-primary); border-color: rgba(8, 247, 254, 0.45); margin-bottom: 2px; }
</style>
