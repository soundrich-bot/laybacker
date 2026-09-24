<script>
  import { FADE_CHOICES, FIX_MODES, UNFIXABLE_MODES, RENAME_RULES, NAME_TOKENS, convertIsSet, chainIsEmpty } from '../chain.js';

  // The Multifunction Chain: one ordered list of steps, run on every file.
  let {
    chain,
    onChange,          // (patch) => void
    presets = [],
    onSavePreset,      // (name) => void
    onLoadPreset,      // (name) => void
    onDeletePreset,    // (name) => void
    onRun,
    running = false,
    progress = { done: 0, total: 0, file: '', step: '' },
    report = [],
    onExportReport,
    fileCount = 0,
    specLabel = '',
    qcMode = 'lufs',
    busy = false,
  } = $props();

  const containers = [
    { value: 'original', label: 'ORIGINAL' }, { value: 'wav', label: 'WAV' }, { value: 'aiff', label: 'AIFF' },
    { value: 'flac', label: 'FLAC' }, { value: 'alac', label: 'ALAC' }, { value: 'aac', label: 'AAC' },
  ];
  const rates = [{ value: '', label: 'ORIGINAL' }, { value: '44100', label: '44.1k' }, { value: '48000', label: '48k' }, { value: '96000', label: '96k' }];
  const depths = [{ value: '', label: 'ORIGINAL' }, { value: '16', label: '16' }, { value: '24', label: '24' }, { value: '32', label: '32f' }];

  let convert = $derived(chain.convert);
  let depthChoices = $derived(depths.filter(d => d.value !== '32' || ['original', 'wav', 'aiff'].includes(convert.container)));
  function setConvert(patch) {
    const next = { ...convert, ...patch };
    if (next.bitDepth === 32 && !['original', 'wav', 'aiff'].includes(next.container)) next.bitDepth = null;
    onChange({ convert: next });
  }
  function setRename(patch) { onChange({ rename: { ...chain.rename, ...patch } }); }

  let empty = $derived(chainIsEmpty(chain));

  // Presets
  let presetName = $state('');
  let showSave = $state(false);
  let selectedPreset = $state('');
  function savePreset() {
    const name = presetName.trim();
    if (!name) return;
    onSavePreset(name);
    selectedPreset = name;
    presetName = '';
    showSave = false;
  }
  function pickPreset(e) {
    const name = e.target.value;
    selectedPreset = name;
    if (name) onLoadPreset(name);
  }

  let done = $derived(report.filter(r => r.status === 'done').length);
  let skipped = $derived(report.filter(r => r.status.startsWith('skipped')).length);
  let failed = $derived(report.filter(r => r.status.startsWith('failed')).length);
  function fmt(n) { return typeof n === 'number' && isFinite(n) ? n.toFixed(1) : '—'; }
</script>

<div class="chain">
  <!-- Presets row -->
  <div class="presets">
    <span class="label" title="Save this chain under a name and recall it later">PRESET</span>
    <select class="sel" value={selectedPreset} onchange={pickPreset} disabled={running}>
      <option value="">— none —</option>
      {#each presets as p}<option value={p.name}>{p.name}</option>{/each}
    </select>
    {#if showSave}
      <input class="name-input" type="text" placeholder="Preset name…" bind:value={presetName}
        onkeydown={(e) => { if (e.key === 'Enter') savePreset(); if (e.key === 'Escape') showSave = false; }} />
      <button class="cap" onclick={savePreset} disabled={!presetName.trim()}>SAVE</button>
      <button class="cap" onclick={() => showSave = false}>CANCEL</button>
    {:else}
      <button class="cap" onclick={() => { showSave = true; presetName = selectedPreset; }} disabled={running} title="Save the current chain as a preset">SAVE AS…</button>
      {#if selectedPreset}
        <button class="cap danger" onclick={() => { onDeletePreset(selectedPreset); selectedPreset = ''; }} disabled={running} title="Delete this preset">DELETE</button>
      {/if}
    {/if}
  </div>

  <!-- Steps -->
  <ol class="steps">
    <li class="step">
      <span class="num">1</span>
      <div class="step-body">
        <div class="step-title">SHAPE <span class="step-hint">runs first, so QC measures the file as it will be delivered</span></div>
        <div class="controls">
          <label class="ctl"><span>FOLD</span>
            <select class="sel" value={chain.fold} onchange={(e) => onChange({ fold: e.target.value })} disabled={running}>
              <option value="none">OFF</option><option value="stereo">→ STEREO</option><option value="mono">→ MONO</option>
            </select></label>
          <label class="ctl"><span>TRIM SILENCE</span>
            <select class="sel" value={chain.trim ? 'on' : 'off'} onchange={(e) => onChange({ trim: e.target.value === 'on' })} disabled={running}>
              <option value="off">OFF</option><option value="on">HEAD & TAIL</option>
            </select></label>
          <label class="ctl"><span>FADE</span>
            <select class="sel" value={String(chain.fade)} onchange={(e) => onChange({ fade: parseFloat(e.target.value) })} disabled={running}>
              {#each FADE_CHOICES as f}<option value={String(f.value)}>{f.label}</option>{/each}
            </select></label>
          <label class="ctl"><span>SPLIT</span>
            <select class="sel" value={chain.split ? 'on' : 'off'} onchange={(e) => onChange({ split: e.target.value === 'on' })} disabled={running}>
              <option value="off">OFF</option><option value="on">→ MONO STEMS</option>
            </select></label>
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">2</span>
      <div class="step-body">
        <div class="step-title">QC <span class="step-hint">against the spec in the QC panel: <strong>{specLabel}</strong>, 6 frames of silence, stereo & phase, plus clicks, clipping and dropouts when those checks are on in the QC panel</span></div>
      </div>
    </li>

    <li class="step">
      <span class="num">3</span>
      <div class="step-body">
        <div class="step-title">6 FRAMES OF SILENCE</div>
        <div class="controls">
          <select class="sel" value={chain.sixFr} onchange={(e) => onChange({ sixFr: e.target.value })} disabled={running}>
            {#each FIX_MODES as m}<option value={m.value}>{m.label}</option>{/each}
          </select>
          <span class="desc">{chain.sixFr === 'prompt' ? 'Asks when sound is found in the first or last 6 frames' : chain.sixFr === 'always' ? 'Mutes the first and last 6 frames on files that fail the check' : 'Checked and reported, never changed'}</span>
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">4</span>
      <div class="step-body">
        <div class="step-title">LOUDNESS</div>
        <div class="controls">
          <select class="sel" value={chain.loudness} onchange={(e) => onChange({ loudness: e.target.value })} disabled={running}>
            {#each FIX_MODES as m}<option value={m.value}>{m.label}</option>{/each}
          </select>
          <span class="desc">{chain.loudness === 'prompt' ? `Asks when a file is off ${specLabel}` : chain.loudness === 'always' ? `Normalises files that are off ${specLabel}; files on spec are left alone` : 'Measured and reported, never changed'}</span>
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">5</span>
      <div class="step-body">
        <div class="step-title">UNFIXABLE ISSUES <span class="step-hint">dual mono, anti-phase, one-sided, a peak that can’t meet the ceiling at target, too short to clock</span></div>
        <div class="controls">
          <select class="sel" value={chain.unfixable} onchange={(e) => onChange({ unfixable: e.target.value })} disabled={running}>
            {#each UNFIXABLE_MODES as m}<option value={m.value}>{m.label}</option>{/each}
          </select>
          <span class="desc">{chain.unfixable === 'prompt' ? 'Warns and lets you pass the file or skip it' : chain.unfixable === 'pass' ? 'Noted in the report; the file goes through' : 'The file is left out of the run, untouched'}</span>
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">6</span>
      <div class="step-body">
        <div class="step-title">CLOCK</div>
        <div class="controls">
          <select class="sel" value={chain.clock ? 'on' : 'off'} onchange={(e) => onChange({ clock: e.target.value === 'on' })} disabled={running}>
            <option value="off">OFF</option><option value="on">ADD CLOCK HANDLES</option>
          </select>
          <span class="desc">10 s of silence at the head, 5 s at the tail</span>
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">7</span>
      <div class="step-body">
        <div class="step-title">EXPORT FILE TYPE</div>
        <div class="controls">
          <label class="ctl"><span>FORMAT</span>
            <select class="sel" class:changed={convert.container !== 'original'} value={convert.container} onchange={(e) => setConvert({ container: e.target.value })} disabled={running}>
              {#each containers as c}<option value={c.value}>{c.label}</option>{/each}
            </select></label>
          {#if convert.container === 'aac'}
            <label class="ctl"><span>BITRATE</span>
              <select class="sel" value={String(convert.aacBitrate)} onchange={(e) => setConvert({ aacBitrate: parseInt(e.target.value) })} disabled={running}>
                <option value="128000">128 kbps</option><option value="192000">192 kbps</option><option value="256000">256 kbps</option><option value="320000">320 kbps</option>
              </select></label>
          {/if}
          <label class="ctl"><span>SAMPLE RATE</span>
            <select class="sel" class:changed={!!convert.sampleRate} value={String(convert.sampleRate ?? '')} onchange={(e) => setConvert({ sampleRate: e.target.value ? parseInt(e.target.value) : null })} disabled={running}>
              {#each rates as r}<option value={r.value}>{r.label}</option>{/each}
            </select></label>
          {#if convert.container !== 'aac'}
            <label class="ctl"><span>BIT DEPTH</span>
              <select class="sel" class:changed={!!convert.bitDepth} value={String(convert.bitDepth ?? '')} onchange={(e) => setConvert({ bitDepth: e.target.value ? parseInt(e.target.value) : null })} disabled={running}>
                {#each depthChoices as d}<option value={d.value}>{d.label}</option>{/each}
              </select></label>
          {/if}
          {#if !convertIsSet(convert)}<span class="desc">Files keep their own format</span>{/if}
        </div>
      </div>
    </li>

    <li class="step">
      <span class="num">8</span>
      <div class="step-body">
        <div class="step-title">RENAME</div>
        <div class="controls">
          <select class="sel" value={chain.rename.rule} onchange={(e) => setRename({ rule: e.target.value })} disabled={running}>
            {#each RENAME_RULES as r}<option value={r.value} title={r.desc}>{r.label}</option>{/each}
          </select>
          {#if chain.rename.rule === 'custom'}
            <input class="pattern" type="text" value={chain.rename.pattern} placeholder="{'{name}_{date}'}"
              oninput={(e) => setRename({ pattern: e.target.value })} disabled={running} spellcheck="false" />
          {:else}
            <span class="desc">{RENAME_RULES.find(r => r.value === chain.rename.rule)?.desc}</span>
          {/if}
        </div>
        {#if chain.rename.rule === 'custom'}
          <div class="tokens">
            {#each NAME_TOKENS as t}
              <button class="token" onclick={() => setRename({ pattern: (chain.rename.pattern || '') + t.token })} title={t.desc} disabled={running}>{t.token}</button>
            {/each}
          </div>
        {/if}
      </div>
    </li>
  </ol>

  <!-- Run -->
  <div class="run-row">
    <button class="run" onclick={onRun} disabled={busy || running || fileCount === 0 || empty}
      title={empty ? 'Every step is off — switch something on' : fileCount === 0 ? 'Drop audio files first' : ''}>
      {#if running}
        RUNNING · FILE {progress.done + 1} OF {progress.total}
      {:else}
        ▶ RUN CHAIN ON {fileCount} FILE{fileCount === 1 ? '' : 'S'}
      {/if}
    </button>
    {#if running}
      <span class="progress-text"><span class="pfile">{progress.file}</span> — {progress.step}</span>
    {:else if report.length}
      <span class="progress-text summary">
        <span class="ok">{done} done</span>{#if skipped} · <span class="warnc">{skipped} skipped</span>{/if}{#if failed} · <span class="bad">{failed} failed</span>{/if}
      </span>
      <button class="cap" onclick={onExportReport} title="Save a CSV of what happened to each file">EXPORT REPORT (CSV)</button>
    {/if}
  </div>

  {#if report.length && !running}
    <div class="report-wrap">
      <table class="report">
        <thead><tr><th>FILE</th><th>RESULT</th><th>QC</th><th class="num">LUFS</th><th class="num">dBTP</th><th>STEPS</th><th>NOTES</th></tr></thead>
        <tbody>
          {#each report as r}
            <tr class:bad={r.status.startsWith('failed')} class:warnrow={r.status.startsWith('skipped')}>
              <td class="file" title={r.file}>{r.file}{#if r.output}<span class="arrow"> → </span><span class="out" title={r.output}>{r.output.split('/').pop()}</span>{/if}</td>
              <td class="status">{r.status.toUpperCase()}</td>
              <td class="qc">
                {#if r.qc}
                  <span class="check" class:pass={r.qc.loudness} class:fail={!r.qc.loudness} title={r.qc.loudness ? 'Loudness on spec' : 'Loudness off spec'}>{r.qc.loudness ? '✓' : '✗'} LEVEL</span>
                  <span class="check" class:pass={r.qc.sixFr} class:fail={!r.qc.sixFr} title={r.qc.sixFr ? 'Head and tail silent' : 'Sound in the first or last 6 frames'}>{r.qc.sixFr ? '✓' : '✗'} 6Fr</span>
                  {#if r.qc.stereo != null}
                    <span class="check" class:pass={r.qc.stereo} class:fail={!r.qc.stereo} title={r.qc.stereo ? 'Genuine stereo, phase coherent' : r.before?.stereo}>{r.qc.stereo ? '✓' : '✗'} STEREO</span>
                  {/if}
                  {#if r.qc.clicks != null}
                    <span class="check" class:pass={r.qc.clicks} class:fail={!r.qc.clicks} title={r.qc.clicks ? 'No digital clicks or edge pops' : 'Possible clicks — see notes; listen back from the QC panel'}>{r.qc.clicks ? '✓' : '✗'} CLICKS</span>
                  {/if}
                  {#if r.qc.clipping != null}
                    <span class="check" class:pass={r.qc.clipping} class:fail={!r.qc.clipping} title={r.qc.clipping ? 'No clipping' : 'Clipping found — see notes'}>{r.qc.clipping ? '✓' : '✗'} CLIP</span>
                  {/if}
                  {#if r.qc.dropouts != null}
                    <span class="check" class:pass={r.qc.dropouts} class:fail={!r.qc.dropouts} title={r.qc.dropouts ? 'No dropouts' : 'Digital silence inside the programme — see notes'}>{r.qc.dropouts ? '✓' : '✗'} DROP</span>
                  {/if}
                {:else}—{/if}
              </td>
              <td class="num">{fmt(r.before?.lufs)}{#if r.after?.lufs != null} → <strong>{fmt(r.after.lufs)}</strong>{/if}</td>
              <td class="num">{fmt(r.before?.tp)}{#if r.after?.tp != null} → <strong>{fmt(r.after.tp)}</strong>{/if}</td>
              <td class="steps-cell">{(r.steps || []).join(', ') || '—'}</td>
              <td class="notes">{[...(r.warnings || []), r.notes].filter(Boolean).join(' · ')}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

<style>
  .chain { display: flex; flex-direction: column; gap: 10px; }
  .label { font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.12em; color: var(--text-secondary); }

  .presets { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .name-input, .pattern {
    font-family: var(--font-mono); font-size: 13px; color: var(--text-primary);
    background: var(--bg-dark); border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    padding: 4px 8px;
  }
  .name-input:focus, .pattern:focus { outline: none; border-color: var(--neon-cyan); }
  .pattern { min-width: 260px; flex: 1; }

  .sel { font-size: 12.5px; font-weight: 700; letter-spacing: 0.04em; padding: 4px 24px 4px 10px; color: var(--text-primary); }
  .sel.changed { color: var(--neon-pink); border-color: rgba(255, 46, 99, 0.45); }
  :global(:root.tame) .sel.changed { color: var(--neon-green); border-color: rgba(90, 138, 122, 0.45); }

  .steps { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
  .step { display: flex; gap: 12px; padding: 8px 4px; border-top: 1px solid var(--border-color); }
  .step:first-child { border-top: none; }
  .num {
    flex-shrink: 0; width: 22px; height: 22px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    font-family: var(--font-display); font-size: 11.5px;
    color: var(--bg-dark); background: var(--neon-cyan);
  }
  .step-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 6px; }
  .step-title { font-family: var(--font-display); font-size: 13px; letter-spacing: 0.1em; color: var(--text-primary); display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; }
  .step-hint { font-family: var(--font-mono); font-size: 12.5px; letter-spacing: normal; color: var(--text-secondary); }
  .step-hint strong { color: var(--neon-cyan); }
  .controls { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .ctl { display: inline-flex; align-items: center; gap: 6px; }
  .ctl span { font-family: var(--font-display); font-size: 11px; letter-spacing: 0.1em; color: var(--text-secondary); }
  .desc { font-family: var(--font-mono); font-size: 12.5px; color: var(--text-secondary); }
  .tokens { display: flex; gap: 4px; flex-wrap: wrap; }
  .token {
    font-family: var(--font-mono); font-size: 12px; color: var(--neon-cyan);
    background: rgba(8, 247, 254, 0.06); border: 1px solid rgba(8, 247, 254, 0.25); border-radius: 3px;
    padding: 1px 6px; cursor: pointer;
  }
  .token:hover { background: rgba(8, 247, 254, 0.14); }

  .cap {
    font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.1em; color: var(--text-muted);
    background: var(--cap-face); border: 1px solid var(--border-color); border-radius: var(--radius-sm);
    padding: 4px 10px; cursor: pointer; transition: all 0.15s; box-shadow: var(--cap-shadow);
  }
  .cap:hover:not(:disabled) { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); box-shadow: var(--cap-shadow-hover); }
  .cap.danger:hover:not(:disabled) { color: var(--neon-pink); border-color: rgba(255, 46, 99, 0.5); }
  .cap:disabled { opacity: 0.4; cursor: not-allowed; }

  .run-row { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; padding-top: 4px; border-top: 1px solid var(--border-color); }
  .run {
    font-family: var(--font-display); font-size: 13.5px; letter-spacing: 0.12em;
    color: var(--bg-dark); background: var(--neon-cyan); border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-sm); padding: 9px 18px; cursor: pointer; box-shadow: var(--cap-shadow);
    transition: all 0.15s;
  }
  .run:hover:not(:disabled) { filter: brightness(1.1); box-shadow: var(--cap-shadow-hover); }
  .run:active:not(:disabled) { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .run:disabled { opacity: 0.4; cursor: not-allowed; }
  .progress-text { font-family: var(--font-mono); font-size: 13px; color: var(--text-secondary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
  .pfile { color: var(--text-secondary); }
  .summary .ok { color: var(--neon-green); font-weight: 700; }
  .summary .warnc { color: var(--neon-yellow); font-weight: 700; }
  .summary .bad { color: var(--neon-orange); font-weight: 700; }

  .report-wrap { max-height: 220px; overflow: auto; border: 1px solid var(--border-color); border-radius: var(--radius-sm); background: var(--bg-dark); }
  .report { width: 100%; border-collapse: collapse; font-family: var(--font-mono); font-size: 13px; }
  .report th { position: sticky; top: 0; background: var(--bg-panel); font-family: var(--font-display); font-size: 11px; letter-spacing: 0.15em; color: var(--text-muted); text-align: left; padding: 5px 8px; border-bottom: 1px solid var(--border-color); }
  .report td { padding: 6px 8px; border-bottom: 1px solid var(--border-color); color: var(--text-primary); vertical-align: top; }
  .report tr:last-child td { border-bottom: none; }
  .report .file { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .report .out { color: var(--neon-cyan); }
  .report .arrow { color: var(--text-muted); }
  .report th.num, .report td.num { text-align: right; white-space: nowrap; }
  .report td.num strong { color: var(--neon-cyan); }
  .report .status { font-family: var(--font-display); font-size: 11px; letter-spacing: 0.1em; color: var(--neon-green); white-space: nowrap; }
  .report tr.warnrow .status { color: var(--neon-yellow); }
  .report tr.bad .status { color: var(--neon-orange); }
  .report .notes { color: var(--text-secondary); }
  .report .qc { white-space: nowrap; }
  .check { display: inline-block; font-family: var(--font-display); font-size: 11px; letter-spacing: 0.1em; margin-right: 8px; }
  .check.pass { color: var(--neon-green); }
  .check.fail { color: var(--neon-orange); }
</style>
