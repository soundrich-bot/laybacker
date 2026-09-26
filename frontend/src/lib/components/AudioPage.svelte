<script>
  import MatchedPairRow from './MatchedPairRow.svelte';
  import QcPlayer from './QcPlayer.svelte';

  // The Audio Only page: everything you can do to audio files on their own,
  // in three sections — QC (measure), File Deliverables (render to a spec),
  // File Processing (change the file's shape; arriving next).
  let {
    pairs = [],
    progressMap = {},
    results = [],
    onUpdateNormalization,
    onUpdateCompliance,
    onUpdateClock,
    onUpdateFilename,
    onRemove,
    onReveal,
    timestampFormat = 'YYYYMMDD_HHmm',
    qcTargetLufs = -23,
    qcTruePeak = -1.0,
    qcMode = 'lufs',
    qcCheckSilence = false,
    qcSpec = 'r128',
    onQcSpecChange,
    qcUnit = 'LUFS',
    qcLufsTol = 1,
    qcCheckClicks = false,
    qcClickSensitivity = 'normal',
    onQcClicksChange,
    onQcClickSensitivityChange,
    qcCheckClipping = false,
    onQcClippingChange,
    qcCheckDropouts = false,
    onQcDropoutsChange,
    qcResults = {},
    qcRunning = false,
    qcProgress = { done: 0, total: 0 },
    onQcTargetChange,
    onQcTruePeakChange,
    onQcModeChange,
    onQcSilenceChange,
    onRunQc,
    onNormalizeAll,
    onClockAll,
    onSixFrAll,
    onApplyNameRule,
    nameRule = 'smart',
    onNameRuleChange,
    isProcessing = false,
    clockChecks = {},
    clockRunning = false,
    clockProgress = { done: 0, total: 0 },
    onRunClockCheck,
    onBackToLayback,
    onSplitAll,
    onJoinAll,
    joinableGroups = [],
    onConvertAll,
    conversionSet = false,
    conversionLabel = '',
    onProcessAll,
    chainSection,       // snippet: the Multifunction Chain panel body
    chainRunning = false,
  } = $props();

  // Fade length per end, chosen next to the FADE ALL button.
  const fadeChoices = [
    { value: 0.01, label: '10 ms' },
    { value: 0.02, label: '20 ms' },
    { value: 0.05, label: '50 ms' },
    { value: 0.1,  label: '100 ms' },
    { value: 0.5,  label: '0.5 s' },
    { value: 1,    label: '1 s' },
    { value: 2,    label: '2 s' },
  ];
  let fadeSecs = $state(0.02);

  let foldable = $derived(pairs.filter(p => (p.audio.channelCount ?? 1) >= 3));
  let monoable = $derived(pairs.filter(p => (p.audio.channelCount ?? 1) >= 2));

  // File Processing: what's in the list that each action can act on.
  let splittable = $derived(pairs.filter(p => (p.audio.channelCount ?? 1) >= 2));
  let joinSummary = $derived(
    joinableGroups.map(g => `${g.stem} → ${g.layout}`).join(', ')
  );

  let busy = $derived(qcRunning || clockRunning || isProcessing || chainRunning);

  // Each section folds away; all of them start open every launch.
  let open = $state({ qc: true, deliver: true, process: true, chain: true });
  function toggle(key) { open[key] = !open[key]; }

  let qcChecked = $derived(Object.values(qcResults).filter(r => !r.error));
  let qcPassCount = $derived(qcChecked.filter(r => r.pass).length);
  let qcSummary = $derived(
    !qcRunning && qcChecked.length > 0 ? `${qcPassCount} of ${qcChecked.length} passed` : ''
  );

  let specLabel = $derived(qcMode === 'peak' ? `${qcTruePeak} dBTP` : `${qcTargetLufs} LUFS`);

  function getResult(pairId) {
    return results.find(r => r.pairId === pairId) ?? null;
  }

  // QC results table: one row per checked file, in list order.
  let qcRows = $derived(
    pairs
      .filter(p => qcResults[p.id])
      .map(p => ({ pair: p, r: qcResults[p.id] }))
  );
  function stereoLabel(st) {
    if (!st) return '—';
    return { stereo: 'STEREO', dual_mono: 'DUAL MONO', anti_phase: 'ANTI-PHASE', one_sided: 'ONE-SIDED',
      mono: 'MONO', multi: `${st.channels} CH`, error: '?' }[st.verdict] ?? st.verdict.toUpperCase();
  }
  function stereoClass(r) {
    const st = r.stereo;
    if (!st) return '';
    if (['anti_phase', 'one_sided'].includes(st.verdict)) return 'bad';
    if (st.verdict === 'dual_mono') return 'warn';
    if (st.verdict === 'stereo') return 'good';
    return '';
  }
  function fmt(n, digits = 1) {
    return typeof n === 'number' && isFinite(n) ? n.toFixed(digits) : '—';
  }

  // Click results: a summary word, and the list to jump the player to.
  function clickSummary(c) {
    if (!c) return '—';
    if (c.error) return '?';
    const bits = [];
    if (c.count > 0) bits.push(`${c.count} CLICK${c.count === 1 ? '' : 'S'}`);
    if (c.headPop) bits.push('HEAD POP');
    if (c.tailPop) bits.push('TAIL POP');
    return bits.length ? bits.join(' · ') : 'NONE';
  }
  function fmtTime(t) {
    const m = Math.floor(t / 60), s = t - m * 60;
    return `${m}:${s.toFixed(3).padStart(6, '0')}`;
  }
  // Listen back: cue the QC player (under the table) a moment before the fault.
  let playerCue = $state(null); // { pairId, time, nonce }
  function listen(pair, time) {
    playerCue = { pairId: pair.id, time, nonce: Date.now() };
  }
  let openClickList = $state(null); // pair id whose click list is expanded
  let openList = $state(null); // `${pairId}:clip` | `${pairId}:drop` — expanded event list
  const CH = ['L', 'R', 'C', 'LFE', 'Ls', 'Rs', 'Lss', 'Rss'];

  // 6 Fr ALL mutes heads and tails — confirm before doing it to every file.
  let showSixFrConfirm = $state(false);
  function confirmSixFrAll() {
    showSixFrConfirm = false;
    onSixFrAll();
  }
</script>

<div class="audio-page">
  <div class="page-head">
    <div class="page-title">
      AUDIO ONLY
      <span class="page-count">{pairs.length} FILE{pairs.length === 1 ? '' : 'S'}</span>
    </div>
    <button class="back-btn" onclick={onBackToLayback} title="Back to the layback page (drop a video to pair with these files)">
      ← LAYBACK
    </button>
  </div>

  <!-- ── QC ── -->
  <section class="panel" class:closed={!open.qc}>
    <button class="panel-head" onclick={() => toggle('qc')} aria-expanded={open.qc}>
      <span class="chevron" class:down={open.qc}><svg width="14" height="14" viewBox="0 0 16 16"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg></span>
      <span class="panel-title">QC</span>
      <span class="panel-desc">Every check runs on every file: level, true peak, stereo & phase, 6 frames of silence, clicks, clipping, dropouts. Nothing is changed — read the list and disregard what doesn't apply.</span>
      {#if !open.qc && qcSummary}<span class="head-summary" class:allpass={qcPassCount === qcChecked.length}>{qcSummary}</span>{/if}
    </button>
    {#if open.qc}
    <div class="panel-row">
      <div class="seg" role="group" aria-label="Reference">
        <button class="seg-btn" class:active={qcMode === 'lufs'} disabled={busy}
          onclick={() => onQcModeChange('lufs')}
          title="Loudness: level to the LUFS target, capped at the true-peak ceiling">LUFS</button>
        <button class="seg-btn" class:active={qcMode === 'peak'} disabled={busy}
          onclick={() => onQcModeChange('peak')}
          title="True Peak: set every file's peak to the dBTP target — loudness ignored">PEAK</button>
      </div>

      <label class="ctl" title="Delivery spec: sets the targets and the tolerances QC judges with. Typing your own numbers makes it CUSTOM.">
        <span class="ctl-label">SPEC</span>
        <select class="sens" value={qcSpec} disabled={busy} onchange={(e) => onQcSpecChange(e.target.value)}>
          <option value="r128">EBU R128</option>
          <option value="a85">ATSC A/85</option>
          <option value="custom">CUSTOM</option>
        </select>
      </label>

      <span class="target" class:dimmed={qcMode === 'peak'}>
        <input class="num" type="number" step="0.5" value={qcTargetLufs}
          disabled={busy || qcMode === 'peak'}
          onchange={(e) => onQcTargetChange(parseFloat(e.target.value))}
          title={qcMode === 'peak' ? 'Loudness is ignored in Peak mode' : `Loudness target for the batch, judged ±${qcLufsTol} LU`} />
        <span class="unit">{qcUnit}</span>
      </span>
      <span class="target" class:primary={qcMode === 'peak'}>
        <input class="num" type="number" step="0.5" value={qcTruePeak} disabled={busy}
          onchange={(e) => onQcTruePeakChange(parseFloat(e.target.value))}
          title={qcMode === 'peak' ? 'True-peak TARGET — every file is boosted or cut to land here' : 'True-peak ceiling — never exceeded'} />
        <span class="unit">dBTP</span>
      </span>

      <label class="ctl" title="How readily a sharp jump counts as a click. Higher finds more, and more false alarms — always listen back.">
        <span class="ctl-label">CLICK SENSITIVITY</span>
        <select class="sens" value={qcClickSensitivity} disabled={busy}
          onchange={(e) => onQcClickSensitivityChange(e.target.value)}>
          <option value="low">LOW</option>
          <option value="normal">NORMAL</option>
          <option value="high">HIGH</option>
        </select>
      </label>

      <button class="cap run" onclick={onRunQc} disabled={busy || pairs.length === 0}>
        {qcRunning ? `CHECKING ${qcProgress.done}/${qcProgress.total}…` : 'RUN QC'}
      </button>

      {#if qcSummary}
        <span class="summary" class:allpass={qcPassCount === qcChecked.length}>{qcSummary}</span>
      {/if}
    </div>

    {#if qcRows.length > 0}
      <div class="qc-table-wrap">
        <table class="qc-table">
          <thead>
            <tr>
              <th class="t-file">FILE</th>
              <th class="t-num">{qcUnit}</th>
              <th class="t-num">dBTP</th>
              <th class="t-tag">STEREO</th>
              <th class="t-tag">6 Fr</th>
              <th class="t-tag">CLICKS</th>
              <th class="t-tag">CLIPPING</th>
              <th class="t-tag">DROPOUTS</th>
              <th class="t-tag">RESULT</th>
            </tr>
          </thead>
          <tbody>
            {#each qcRows as { pair, r } (pair.id)}
              {#if r.error}
                <tr class="row-fail">
                  <td class="t-file" title={pair.audio.path}>{pair.audio.filename}</td>
                  <td class="t-num" colspan="7">{r.error}</td>
                  <td class="t-tag"><span class="verdict fail">ERROR</span></td>
                </tr>
              {:else}
                <tr class:row-fail={!r.pass}>
                  <td class="t-file" title={pair.audio.path}>{pair.audio.filename}</td>
                  <td class="t-num" class:bad={!r.lufsPass} class:dim={r.mode === 'peak'}
                    title={r.mode === 'peak' ? 'Loudness is not judged in Peak mode' : `Target ${qcTargetLufs} ${qcUnit} (±${qcLufsTol} LU)`}>
                    {fmt(r.measuredLufs)}
                  </td>
                  <td class="t-num" class:bad={!r.peakPass}
                    title={r.mode === 'peak' ? `Target ${qcTruePeak} dBTP` : `Ceiling ${r.peakLimit} dBTP`}>
                    {fmt(r.measuredTP)}
                  </td>
                  <td class="t-tag"><span class="tag {stereoClass(r)}"
                    title={r.stereo?.correlation != null ? `Phase correlation ${r.stereo.correlation.toFixed(2)}` : ''}>{stereoLabel(r.stereo)}</span></td>
                  <td class="t-tag"><span class="tag" class:good={r.silencePass} class:bad={!r.silencePass}>{r.silencePass ? 'SILENT' : (r.headHasAudio && r.tailHasAudio ? 'HEAD & TAIL' : r.headHasAudio ? 'HEAD' : 'TAIL')}</span></td>
                  <td class="t-tag clicks-cell">
                    {#if r.clicks?.error}
                      <span class="tag warn" title={r.clicks.error}>?</span>
                    {:else if r.clicksPass}
                      <span class="tag good">NONE</span>
                    {:else if r.clicks}
                      <button class="tag bad linkish" onclick={() => openClickList = openClickList === pair.id ? null : pair.id}
                        title="Show where — click a time to listen back">{clickSummary(r.clicks)} ▾</button>
                      {#if openClickList === pair.id}
                        <div class="click-list">
                          {#if r.clicks.headPop}<button class="click-time" onclick={() => listen(pair, 0)}>head pop · 0:00.000</button>{/if}
                          {#each r.clicks.clicks as c}
                            <button class="click-time" onclick={() => listen(pair, c.time)}
                              title="{c.prominenceDb.toFixed(0)} dB above the surrounding audio, {c.widthMs.toFixed(2)} ms wide">
                              {fmtTime(c.time)}{r.clicks.channels > 1 ? ` ${c.allChannels ? (r.clicks.channels === 2 ? 'L+R' : 'ALL') : (CH[c.channel] ?? 'ch' + (c.channel + 1))}` : ''}
                            </button>
                          {/each}
                          {#if r.clicks.count > r.clicks.clicks.length}<span class="click-more">…and {r.clicks.count - r.clicks.clicks.length} more</span>{/if}
                          {#if r.clicks.tailPop}<button class="click-time" onclick={() => listen(pair, Math.max(0, pair.audio.durationSecs - 2))}>tail pop · end</button>{/if}
                        </div>
                      {/if}
                    {:else}
                      <span class="tag">—</span>
                    {/if}
                  </td>
                  <td class="t-tag clicks-cell">
                    {#if r.clipping?.error}
                      <span class="tag warn" title={r.clipping.error}>?</span>
                    {:else if r.clippingPass}
                      <span class="tag good">NONE</span>
                    {:else if r.clipping}
                      <button class="tag bad linkish" onclick={() => openList = openList === `${pair.id}:clip` ? null : `${pair.id}:clip`}
                        title="{r.clipping.clippedSamples} clipped samples, longest run {r.clipping.longestRun} — click a time to listen back">
                        {r.clipping.count} RUN{r.clipping.count === 1 ? '' : 'S'} ▾</button>
                      {#if openList === `${pair.id}:clip`}
                        <div class="click-list">
                          {#each r.clipping.events as e}
                            <button class="click-time" onclick={() => listen(pair, e.time)}
                              title="{e.kind === 'flat' ? 'Flat top' : 'At full scale'} at {e.levelDb.toFixed(1)} dBFS, {e.samples} samples">
                              {fmtTime(e.time)}{(pair.audio.channelCount ?? 1) > 1 ? ` ${CH[e.channel] ?? 'ch' + (e.channel + 1)}` : ''}{e.kind === 'flat' ? ' ▭' : ''}
                            </button>
                          {/each}
                          {#if r.clipping.count > r.clipping.events.length}<span class="click-more">…and {r.clipping.count - r.clipping.events.length} more</span>{/if}
                        </div>
                      {/if}
                    {:else}
                      <span class="tag">—</span>
                    {/if}
                  </td>
                  <td class="t-tag clicks-cell">
                    {#if r.dropouts?.error}
                      <span class="tag warn" title={r.dropouts.error}>?</span>
                    {:else if r.dropoutsPass}
                      <span class="tag good">NONE</span>
                    {:else if r.dropouts}
                      <button class="tag bad linkish" onclick={() => openList = openList === `${pair.id}:drop` ? null : `${pair.id}:drop`}
                        title="{r.dropouts.totalSecs.toFixed(2)} s of silence inside the programme — click a time to listen back">
                        {r.dropouts.count} GAP{r.dropouts.count === 1 ? '' : 'S'} ▾</button>
                      {#if openList === `${pair.id}:drop`}
                        <div class="click-list">
                          {#each r.dropouts.gaps as g}
                            <button class="click-time" onclick={() => listen(pair, g.start)} title="{(g.duration * 1000).toFixed(0)} ms of silence">
                              {fmtTime(g.start)} · {g.duration >= 1 ? g.duration.toFixed(2) + ' s' : (g.duration * 1000).toFixed(0) + ' ms'}
                            </button>
                          {/each}
                          {#if r.dropouts.count > r.dropouts.gaps.length}<span class="click-more">…and {r.dropouts.count - r.dropouts.gaps.length} more</span>{/if}
                        </div>
                      {/if}
                    {:else}
                      <span class="tag">—</span>
                    {/if}
                  </td>
                  <td class="t-tag"><span class="verdict" class:pass={r.pass} class:fail={!r.pass}>{r.pass ? 'PASS' : 'FAIL'}</span></td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {#if pairs.length > 0}
      <QcPlayer {pairs} {qcResults} cue={playerCue} />
    {/if}
    {/if}
  </section>

  <!-- ── FILE DELIVERABLES ── -->
  <section class="panel" class:closed={!open.deliver}>
    <button class="panel-head" onclick={() => toggle('deliver')} aria-expanded={open.deliver}>
      <span class="chevron" class:down={open.deliver}><svg width="14" height="14" viewBox="0 0 16 16"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg></span>
      <span class="panel-title">FILE DELIVERABLES</span>
      <span class="panel-desc">Render new files to a spec. Each runs now, on every file, then re-measures. Originals are never touched.</span>
    </button>
    {#if open.deliver}
    <div class="actions">
      <button class="action normalise" onclick={onNormalizeAll} disabled={busy || pairs.length === 0}>
        <span class="action-title">{isProcessing ? 'WORKING…' : `NORMALISE ALL → ${specLabel}`}</span>
        <span class="action-desc">{qcMode === 'peak' ? 'Set every file’s true peak to the target' : 'Level every file to the loudness target'}</span>
      </button>
      <button class="action sixfr" onclick={() => showSixFrConfirm = true} disabled={busy || pairs.length === 0}>
        <span class="action-title">6 Fr ALL</span>
        <span class="action-desc">6 frames of digital silence at head and tail (UK broadcast)</span>
      </button>
      <button class="action clock" onclick={onClockAll} disabled={busy || pairs.length === 0}>
        <span class="action-title">{clockRunning ? `CHECKING ${clockProgress.done}/${clockProgress.total}…` : 'CLOCK ALL'}</span>
        <span class="action-desc">10s silence at the head, 5s at the tail — for files that pass the check</span>
      </button>
      <button class="action convert" onclick={onConvertAll} disabled={busy || pairs.length === 0 || !conversionSet}
        title={conversionSet ? `Render every file as ${conversionLabel}` : 'Choose a format, sample rate or bit depth in the output bar below'}>
        <span class="action-title">{isProcessing ? 'WORKING…' : conversionSet ? `CONVERT ALL → ${conversionLabel}` : 'CONVERT ALL'}</span>
        <span class="action-desc">
          {#if conversionSet}
            Every file re-rendered as {conversionLabel} — set in the output bar below
          {:else}
            Pick a format, sample rate or bit depth in the output bar below
          {/if}
        </span>
      </button>
    </div>
    {/if}
  </section>

  <!-- ── FILE PROCESSING ── -->
  <section class="panel" class:closed={!open.process}>
    <button class="panel-head" onclick={() => toggle('process')} aria-expanded={open.process}>
      <span class="chevron" class:down={open.process}><svg width="14" height="14" viewBox="0 0 16 16"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg></span>
      <span class="panel-title">FILE PROCESSING</span>
      <span class="panel-desc">Change the file’s shape. New files are written beside the originals; the list reloads with the results.</span>
    </button>
    {#if open.process}
    <div class="actions">
      <button class="action split" onclick={onSplitAll} disabled={busy || splittable.length === 0}
        title={splittable.length ? `${splittable.length} multichannel file${splittable.length === 1 ? '' : 's'} in the list` : 'No multichannel files in the list'}>
        <span class="action-title">{isProcessing ? 'WORKING…' : 'SPLIT ALL → MONO'}</span>
        <span class="action-desc">
          {#if splittable.length}
            {splittable.length} file{splittable.length === 1 ? '' : 's'} → one mono file per channel, named by channel (_L, _R, _C, _LFE, _Ls, _Rs)
          {:else}
            Multichannel files → one mono file per channel (_L, _R, _C, _LFE, _Ls, _Rs)
          {/if}
        </span>
      </button>
      <button class="action join" onclick={onJoinAll} disabled={busy || joinableGroups.length === 0}
        title={joinableGroups.length ? joinSummary : 'Drop matching mono files (e.g. Mix_L.wav + Mix_R.wav, or a full _L _R _C _LFE _Ls _Rs set)'}>
        <span class="action-title">{isProcessing ? 'WORKING…' : 'JOIN MONOS → POLY'}</span>
        <span class="action-desc">
          {#if joinableGroups.length}
            {joinableGroups.length} set{joinableGroups.length === 1 ? '' : 's'} ready: {joinSummary}
          {:else}
            Matching mono files (_L _R, or a full 5.1 / 7.1 set) → one stereo, 5.1 or 7.1 file
          {/if}
        </span>
      </button>
      <button class="action fold" onclick={() => onProcessAll('fold_stereo')} disabled={busy || foldable.length === 0}
        title={foldable.length ? `${foldable.length} surround file${foldable.length === 1 ? '' : 's'} in the list` : 'No surround (5.1 / 7.1) files in the list'}>
        <span class="action-title">{isProcessing ? 'WORKING…' : 'FOLD ALL → STEREO'}</span>
        <span class="action-desc">5.1 / 7.1 → Lo/Ro stereo: centre and surrounds at −3 dB, LFE dropped, peak-limited so it can't clip</span>
      </button>
      <button class="action fold" onclick={() => onProcessAll('fold_mono')} disabled={busy || monoable.length === 0}
        title={monoable.length ? `${monoable.length} stereo / multichannel file${monoable.length === 1 ? '' : 's'} in the list` : 'Every file is already mono'}>
        <span class="action-title">{isProcessing ? 'WORKING…' : 'FOLD ALL → MONO'}</span>
        <span class="action-desc">Stereo or surround → one mono file (L+R halved, so a dual-mono file keeps its level)</span>
      </button>
      <div class="action-with-opt">
        <button class="action fade" onclick={() => onProcessAll('fade', fadeSecs)} disabled={busy || pairs.length === 0}>
          <span class="action-title">{isProcessing ? 'WORKING…' : 'FADE ALL'}</span>
          <span class="action-desc">Fade in at the head and out at the tail — short fades kill clicks</span>
        </button>
        <select class="opt-select" bind:value={fadeSecs} disabled={busy} title="Fade length at each end">
          {#each fadeChoices as f}
            <option value={f.value}>{f.label}</option>
          {/each}
        </select>
      </div>
      <button class="action trim" onclick={() => onProcessAll('trim')} disabled={busy || pairs.length === 0}>
        <span class="action-title">{isProcessing ? 'WORKING…' : 'TRIM SILENCE ALL'}</span>
        <span class="action-desc">Cut silence (below −60 dB) off the head and tail, keeping 10 ms either side</span>
      </button>
    </div>
    {/if}
  </section>

  <!-- ── MULTIFUNCTION CHAIN ── -->
  {#if chainSection}
  <section class="panel chain-panel" class:closed={!open.chain}>
    <button class="panel-head" onclick={() => toggle('chain')} aria-expanded={open.chain}>
      <span class="chevron" class:down={open.chain}><svg width="14" height="14" viewBox="0 0 16 16"><path d="M4 2L14 8L4 14V2Z" fill="currentColor"/></svg></span>
      <span class="panel-title">MULTIFUNCTION CHAIN</span>
      <span class="panel-desc">One ordered run of steps on every file: shape → QC → fixes (with prompts) → clock → export type → rename. Save it as a preset.</span>
    </button>
    {#if open.chain}
      {@render chainSection()}
    {/if}
  </section>
  {/if}

  <!-- ── Files ── -->
  <div class="list-head">
    <span class="col-label">AUDIO</span>
    {#if onNameRuleChange}
      <div class="name-rule" role="group" aria-label="Name all files by">
        <span class="name-rule-label" title="How every output file is named. Files you rename by hand keep their name.">NAME ALL BY</span>
        <button class="cap small" class:active={nameRule === 'smart'} onclick={() => onNameRuleChange('smart')}
          title="The file's own name">SMART</button>
        <button class="cap small" class:active={nameRule === 'audio'} onclick={() => onNameRuleChange('audio')}
          title="Each output takes its audio file's name">AUDIO</button>
        {#if nameRule !== 'smart'}
          <button class="cap small cancel" onclick={() => onNameRuleChange('smart')}
            title="Cancel the batch naming rule">✕ CANCEL</button>
        {/if}
      </div>
    {/if}
  </div>

  {#if pairs.length === 0}
    <div class="empty">
      <p class="empty-text">NO AUDIO FILES</p>
      <p class="empty-hint">Drop audio files above to get started</p>
    </div>
  {:else}
    <div class="pairs-list">
      {#each pairs as pair (pair.id)}
        <MatchedPairRow
          {pair}
          progress={progressMap[pair.id] ?? null}
          result={getResult(pair.id)}
          qcResult={qcResults[pair.id] ?? null}
          {onUpdateNormalization}
          {onUpdateCompliance}
          {onUpdateClock}
          {onRunClockCheck}
          clockCheck={clockChecks[pair.id] ?? null}
          {onUpdateFilename}
          {onRemove}
          {onReveal}
          {onApplyNameRule}
          {timestampFormat}
        />
      {/each}
    </div>
  {/if}
</div>

{#if showSixFrConfirm}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="confirm-overlay" onclick={() => showSixFrConfirm = false}>
    <div class="confirm-box" onclick={(e) => e.stopPropagation()} role="alertdialog" aria-modal="true">
      <div class="confirm-title">APPLY 6 Fr TO ALL {pairs.length} FILE{pairs.length !== 1 ? 'S' : ''}?</div>
      <p class="confirm-body">
        The first and last 6 frames (240&nbsp;ms) of <strong>every file</strong> will be forced to
        digital silence, with a short fade to prevent clicks. <strong>Any sound in those regions
        will be muted</strong> in the new files. Your originals are untouched.
      </p>
      <div class="confirm-actions">
        <button class="confirm-cancel" onclick={() => showSixFrConfirm = false}>CANCEL</button>
        <button class="confirm-apply" onclick={confirmSixFrAll}>YES — APPLY 6 Fr TO ALL</button>
      </div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && showSixFrConfirm) showSixFrConfirm = false; }} />

<style>
  .audio-page {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    padding: 0 var(--gap-lg) var(--gap-lg);
    /* The page itself scrolls, inside the space above the output bar — so
       nothing ever slides behind it. */
    overflow-y: auto;
  }

  .page-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--gap-sm) 0 2px;
  }
  .page-title {
    font-family: var(--font-display);
    font-size: 14px;
    letter-spacing: 0.15em;
    color: var(--neon-cyan);
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .page-count {
    font-family: var(--font-mono);
    font-size: 12.5px;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }
  .back-btn {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 10px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .back-btn:hover { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); box-shadow: var(--cap-shadow-hover); }
  .back-btn:active { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }

  /* ── Sections ── */
  .panel {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 8px 12px 10px;
    flex-shrink: 0;
  }
  .panel.coming { opacity: 0.6; }
  .panel.closed { padding-bottom: 8px; }
  .panel.chain-panel { border-color: rgba(8, 247, 254, 0.35); }
  .panel-head {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 8px;
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: inherit;
  }
  .panel.closed .panel-head { margin-bottom: 0; }
  .panel-head:hover .panel-title { text-shadow: 0 0 8px rgba(8, 247, 254, 0.5); }
  /* Fold indicator: just a big triangle, in the title colour. */
  .chevron {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    color: var(--neon-cyan);
    flex-shrink: 0;
  }
  .chevron svg { width: 18px; height: 18px; transition: transform 0.15s; }
  .chevron.down svg { transform: rotate(90deg); }
  .panel-head:hover .chevron { filter: brightness(1.15); }
  .head-summary { margin-left: auto; font-family: var(--font-mono); font-size: 13px; font-weight: 700; color: var(--neon-orange); }
  .head-summary.allpass { color: var(--neon-green); }
  .panel-title {
    font-family: var(--font-display);
    font-size: 13px;
    letter-spacing: 0.15em;
    color: var(--neon-cyan);
  }
  .panel-desc {
    font-family: var(--font-mono);
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-secondary);
  }
  .panel-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--gap-sm);
  }

  /* ── Key-caps ── */
  .seg { display: inline-flex; border: 1px solid var(--border-color); border-radius: var(--radius-sm); overflow: hidden; box-shadow: var(--cap-shadow); }
  .seg-btn {
    font-family: var(--font-display);
    font-size: 11.5px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: none;
    padding: 4px 9px;
    cursor: pointer;
    transition: color 0.15s, background 0.15s;
  }
  .seg-btn + .seg-btn { border-left: 1px solid var(--border-color); }
  .seg-btn:hover:not(:disabled):not(.active) { color: var(--neon-cyan); }
  .seg-btn.active { color: var(--bg-dark); background: var(--neon-cyan); }
  .seg-btn:disabled { opacity: 0.4; cursor: not-allowed; }

  .target { display: flex; align-items: center; gap: 4px; transition: opacity 0.15s; }
  .target.dimmed { opacity: 0.4; }
  .target.primary .unit { color: var(--neon-cyan); }
  .num {
    width: 68px;
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 3px 6px;
  }
  .num:focus { outline: none; border-color: var(--neon-cyan); }
  .unit { font-family: var(--font-mono); font-size: 12px; color: var(--text-secondary); }

  .cap {
    font-family: var(--font-display);
    font-size: 11.5px;
    letter-spacing: 0.1em;
    color: var(--text-muted);
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 4px 10px;
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .cap.small { padding: 2px 8px; }
  .cap.run { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); }
  .cap.cancel { color: var(--neon-orange); border-color: rgba(255, 149, 0, 0.45); margin-left: 4px; }
  .cap:hover:not(:disabled):not(.active) { color: var(--neon-cyan); border-color: rgba(8, 247, 254, 0.5); box-shadow: var(--cap-shadow-hover); }
  .cap.cancel:hover { color: var(--neon-orange); border-color: var(--neon-orange); }
  .cap:active:not(:disabled) { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .cap.active { color: var(--bg-dark); background: var(--neon-cyan); border-color: var(--neon-cyan); }
  .cap:disabled { opacity: 0.4; cursor: not-allowed; transform: none; }

  .summary { font-family: var(--font-mono); font-size: 13px; font-weight: 700; color: var(--neon-orange); margin-left: auto; }
  .summary.allpass { color: var(--neon-green); }

  /* ── QC results table: the numbers, big and in one place ── */
  .qc-table-wrap {
    margin-top: 8px;
    max-height: 200px;
    overflow: auto;
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
  }
  .qc-table { width: 100%; border-collapse: collapse; font-family: var(--font-mono); }
  .qc-table th {
    position: sticky; top: 0;
    background: var(--bg-panel);
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.15em;
    color: var(--text-muted);
    text-align: left;
    padding: 5px 10px;
    border-bottom: 1px solid var(--border-color);
  }
  .qc-table td { padding: 6px 10px; border-bottom: 1px solid var(--border-color); font-size: 13px; color: var(--text-primary); }
  .qc-table tr:last-child td { border-bottom: none; }
  .qc-table .t-file { max-width: 0; width: 46%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .qc-table th.t-num, .qc-table td.t-num { text-align: right; }
  .qc-table td.t-num { font-size: 17px; font-weight: 700; color: var(--neon-cyan); white-space: nowrap; }
  .qc-table td.t-num.dim { color: var(--text-muted); font-weight: 400; }
  .qc-table td.t-num.bad { color: var(--neon-orange); }
  .qc-table .t-tag { white-space: nowrap; }
  .qc-table .row-fail td { background: rgba(255, 149, 0, 0.05); }
  .tag { font-family: var(--font-display); font-size: 11px; letter-spacing: 0.08em; color: var(--text-secondary); }
  .tag.good { color: var(--neon-green); }
  .tag.warn { color: var(--neon-yellow); }
  .tag.bad { color: var(--neon-orange); }
  .sens { font-size: 12px; font-weight: 700; letter-spacing: 0.06em; padding: 4px 24px 4px 10px; color: var(--text-primary); }
  .ctl { display: inline-flex; align-items: center; gap: 6px; }
  .ctl-label { font-family: var(--font-display); font-size: 11px; letter-spacing: 0.1em; color: var(--text-secondary); }
  .linkish { background: none; border: none; padding: 0; cursor: pointer; }
  .linkish:hover { text-decoration: underline; }
  .clicks-cell { position: relative; }
  .click-list { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; max-width: 360px; }
  .click-time {
    font-family: var(--font-mono); font-size: 12px; color: var(--neon-cyan);
    background: rgba(8, 247, 254, 0.06); border: 1px solid rgba(8, 247, 254, 0.3); border-radius: 3px;
    padding: 1px 6px; cursor: pointer;
  }
  .click-time:hover { background: rgba(8, 247, 254, 0.16); }
  .click-more { font-family: var(--font-mono); font-size: 12px; color: var(--text-muted); align-self: center; }
  .verdict {
    display: inline-block;
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.12em;
    padding: 2px 8px;
    border-radius: 3px;
    border: 1px solid currentColor;
  }
  .verdict.pass { color: var(--neon-green); background: rgba(57, 255, 20, 0.08); }
  .verdict.fail { color: var(--neon-orange); background: rgba(255, 149, 0, 0.1); }

  /* ── Deliverable actions: big labelled buttons ── */
  .actions {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--gap-sm);
  }
  .action {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    text-align: left;
    padding: 9px 12px;
    background: var(--cap-face);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.15s;
    box-shadow: var(--cap-shadow);
  }
  .action-title { font-family: var(--font-display); font-size: 13px; letter-spacing: 0.08em; }
  .action-desc { font-family: var(--font-mono); font-size: 12.5px; color: var(--text-secondary); line-height: 1.45; }
  .action.normalise .action-title { color: var(--neon-yellow); }
  .action.sixfr .action-title { color: var(--neon-orange); }
  .action.clock .action-title { color: var(--neon-cyan); }
  .action.split .action-title,
  .action.join .action-title,
  .action.fold .action-title,
  .action.fade .action-title,
  .action.trim .action-title { color: var(--neon-green); }
  .action.convert .action-title { color: var(--neon-pink); }
  .action.split:hover:not(:disabled),
  .action.join:hover:not(:disabled),
  .action.fold:hover:not(:disabled),
  .action.fade:hover:not(:disabled),
  .action.trim:hover:not(:disabled) { border-color: var(--neon-green); box-shadow: var(--cap-shadow-hover); }
  .action.convert:hover:not(:disabled) { border-color: var(--neon-pink); box-shadow: var(--cap-shadow-hover); }

  /* An action with a small option control beside it (FADE ALL + length) */
  .action-with-opt { display: flex; gap: 4px; align-items: stretch; }
  .action-with-opt .action { flex: 1; }
  .opt-select {
    font-family: var(--font-mono);
    font-size: 12.5px;
    font-weight: 700;
    color: var(--text-secondary);
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-sm);
    padding: 0 6px;
    width: 72px;
  }
  .action.normalise:hover:not(:disabled) { border-color: var(--neon-yellow); box-shadow: var(--cap-shadow-hover); }
  .action.sixfr:hover:not(:disabled) { border-color: var(--neon-orange); box-shadow: var(--cap-shadow-hover); }
  .action.clock:hover:not(:disabled) { border-color: var(--neon-cyan); box-shadow: var(--cap-shadow-hover); }
  .action:active:not(:disabled) { transform: translateY(1px); box-shadow: var(--cap-shadow-pressed); }
  .action:disabled { opacity: 0.4; cursor: not-allowed; }

  /* ── Files ── */
  .list-head {
    display: flex;
    align-items: center;
    padding: 4px var(--gap-md) 0;
    gap: var(--gap-sm);
  }
  .col-label { flex: 1; font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.12em; color: var(--text-secondary); }
  .name-rule { display: inline-flex; align-items: center; gap: 4px; }
  .name-rule-label { font-family: var(--font-display); font-size: 11.5px; letter-spacing: 0.12em; color: var(--text-secondary); margin-right: 4px; cursor: help; }

  .pairs-list {
    flex: none;
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
    padding-bottom: var(--gap-md);
  }

  .empty { flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center; opacity: 0.5; }
  .empty-text { font-family: var(--font-display); font-size: 15px; letter-spacing: 0.15em; color: var(--text-muted); margin-bottom: var(--gap-xs); }
  .empty-hint { font-family: var(--font-mono); font-size: 13px; color: var(--text-muted); }

  /* ── 6 Fr ALL confirm ── */
  .confirm-overlay { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: flex; align-items: center; justify-content: center; z-index: 100; }
  .confirm-box { width: min(460px, calc(100vw - 48px)); background: var(--bg-panel); border: 1px solid var(--neon-orange); border-radius: var(--radius-md); padding: var(--gap-lg); box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5); }
  .confirm-title { font-family: var(--font-display); font-size: 14px; letter-spacing: 0.08em; color: var(--neon-orange); margin-bottom: var(--gap-md); }
  .confirm-body { font-family: var(--font-mono); font-size: 12px; line-height: 1.6; color: var(--text-secondary); margin-bottom: var(--gap-lg); }
  .confirm-body strong { color: var(--text-primary); }
  .confirm-actions { display: flex; justify-content: flex-end; gap: var(--gap-sm); }
  .confirm-cancel { font-family: var(--font-mono); font-size: 11px; font-weight: 700; letter-spacing: 0.05em; color: var(--text-muted); background: var(--cap-face); border: 1px solid var(--border-color); border-radius: var(--radius-sm); padding: 7px 14px; cursor: pointer; box-shadow: var(--cap-shadow); }
  .confirm-cancel:hover { color: var(--text-primary); box-shadow: var(--cap-shadow-hover); }
  .confirm-apply { font-family: var(--font-display); font-size: 11px; letter-spacing: 0.08em; color: var(--bg-dark); background: var(--neon-orange); border: 1px solid var(--neon-orange); border-radius: var(--radius-sm); padding: 7px 14px; cursor: pointer; box-shadow: var(--cap-shadow); }
  .confirm-apply:hover { filter: brightness(1.1); box-shadow: var(--cap-shadow-hover); }
</style>
