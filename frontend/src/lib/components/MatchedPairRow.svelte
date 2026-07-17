<script>
  import MediaPreview from './MediaPreview.svelte';
  import ProResButton from './ProResButton.svelte';

  import { invoke } from '@tauri-apps/api/core';

  let {
    pair,
    progress = null,
    result = null,
    qcResult = null,
    onUpdateNormalization,
    onUpdateFilename,
    onUpdateCompliance,
    onUpdateClock,
    onRunClockCheck,
    clockCheck = null,
    onRemove,
    onReveal,
    onCreateProres,
    timestampFormat = 'YYYYMMDD_HHmm',
  } = $props();

  let showNormSettings = $state(false);
  let editingName = $state(false);
  let editName = $state('');
  let previewFile = $state(null); // { path, filename, mediaType }
  let silenceCheck = $state(null); // { headHasAudio, tailHasAudio, headPeak, tailPeak }
  let checkingSilence = $state(false);
  let showSilenceDetail = $state(false); // expand the 6 Fr warning into a detail panel
  let showDurationDetail = $state(false); // expand the duration-mismatch warning

  // Clock (audio-only): the 10s/5s handles are gated behind a levels +
  // head/tail-silence check. The check itself lives in the store, so this button
  // and the batch CLOCK ALL pass share one path — `clockCheck` arrives as a prop.
  let checkingClock = $state(false);
  let showClockDetail = $state(false);
  let clockFailed = $derived(
    !!clockCheck && !clockCheck.error && (!clockCheck.silencePass || !clockCheck.loudnessPass)
  );

  async function toggleClock() {
    if (pair.clockEnabled) {
      onUpdateClock(pair.id, false);
      return;
    }
    checkingClock = true;
    try {
      const passed = await onRunClockCheck(pair.id);
      showClockDetail = !passed; // on fail, reveal why (with proceed-anyway)
    } finally {
      checkingClock = false;
    }
  }

  // Override: clock the file even though the check didn't pass.
  function proceedClock() {
    onUpdateClock(pair.id, true);
    showClockDetail = false;
  }

  // Batch QC: why a file failed, and the one-click fix (mark it for the export).
  let qcReason = $derived.by(() => {
    if (!qcResult || qcResult.error || qcResult.pass) return '';
    const bits = [];
    if (!qcResult.lufsPass) bits.push(`level is ${qcResult.measuredLufs.toFixed(1)} LUFS`);
    if (!qcResult.peakPass) bits.push(`true peak ${qcResult.measuredTP.toFixed(1)} dBTP is over the ${qcResult.peakLimit} dBTP ceiling`);
    if (!qcResult.silencePass) {
      const where = qcResult.headHasAudio && qcResult.tailHasAudio ? 'head & tail'
        : qcResult.headHasAudio ? 'head' : 'tail';
      bits.push(`audio in the ${where} silence guard`);
    }
    return bits.join('; ');
  });

  function fixQc() {
    // Mark it for correction: NORM is already retargeted to the batch value.
    onUpdateNormalization(pair.id, true, pair.normalizationSettings);
  }

  // Split the output filename so the extension (.mov / .mp4 / .wav) is always
  // shown and highlighted — long names truncate in the middle, never hiding it.
  let outputDot = $derived(pair.outputFilename.lastIndexOf('.'));
  let outputStem = $derived(outputDot > 0 ? pair.outputFilename.slice(0, outputDot) : pair.outputFilename);
  let outputExt = $derived(outputDot > 0 ? pair.outputFilename.slice(outputDot + 1) : '');

  // Norm mode: 'broadcast' (LUFS target) or 'fullscale' (true peak only)
  // We infer from settings: if targetLufs is 0 or very high, it's full scale
  let normMode = $state('fullscale');

  function startEditing() {
    editName = pair.outputFilename;
    editingName = true;
  }

  function toggleNorm() {
    // Audio-only: NORM is a plain on/off at the batch target — there's no
    // per-file settings menu to reveal (the QC bar owns the loudness value).
    if (isAudioOnly) {
      onUpdateNormalization(pair.id, !pair.normalizationEnabled, pair.normalizationSettings);
      return;
    }
    // Video pairs keep the per-file menu. Norm can be enabled with the menu
    // collapsed; in that case the first click reveals the menu rather than
    // disabling norm — otherwise it takes two clicks to reach the options.
    if (pair.normalizationEnabled && !showNormSettings) {
      showNormSettings = true;
      return;
    }
    const newEnabled = !pair.normalizationEnabled;
    onUpdateNormalization(pair.id, newEnabled, pair.normalizationSettings);
    showNormSettings = newEnabled;
  }

  function saveName() {
    if (editName.trim() && editName !== pair.outputFilename) {
      onUpdateFilename(pair.id, editName.trim());
    }
    editingName = false;
  }

  function cancelEdit() {
    editingName = false;
  }

  function addTimestamp() {
    const now = new Date();
    const pad = (n) => String(n).padStart(2, '0');
    const Y = now.getFullYear();
    const M = pad(now.getMonth() + 1);
    const D = pad(now.getDate());
    const h = pad(now.getHours());
    const m = pad(now.getMinutes());

    let stamp;
    switch (timestampFormat) {
      case 'YYYY-MM-DD_HH-mm': stamp = `${Y}-${M}-${D}_${h}-${m}`; break;
      case 'DD-MM-YYYY_HH-mm': stamp = `${D}-${M}-${Y}_${h}-${m}`; break;
      case 'MMDDYYYY_HHmm':    stamp = `${M}${D}${Y}_${h}${m}`; break;
      default:                  stamp = `${Y}${M}${D}_${h}${m}`; break;
    }

    const name = pair.outputFilename;
    const dotIdx = name.lastIndexOf('.');
    const base = dotIdx > 0 ? name.slice(0, dotIdx) : name;
    const ext = dotIdx > 0 ? name.slice(dotIdx) : '';
    onUpdateFilename(pair.id, `${base}_${stamp}${ext}`);
  }

  function formatDuration(secs) {
    const m = Math.floor(secs / 60);
    const s = Math.floor(secs % 60);
    return `${m}:${s.toString().padStart(2, '0')}`;
  }

  let progressPct = $derived(progress ? Math.round(progress.progress * 100) : 0);
  let isComplete = $derived(result?.success === true);
  let isFailed = $derived(result?.success === false);

  let isAudioOnly = $derived(!pair.video);

  async function toggleCompliance() {
    const newEnabled = !pair.silenceCompliance;
    onUpdateCompliance(pair.id, newEnabled);
    if (newEnabled && !silenceCheck) {
      await runSilenceCheck();
    }
  }

  async function runSilenceCheck() {
    checkingSilence = true;
    try {
      const [headHasAudio, tailHasAudio, headPeak, tailPeak] = await invoke('check_silence', {
        audioPath: pair.audio.path,
        durationSecs: pair.audio.durationSecs,
        silenceMs: pair.silenceMs ?? 240.0,
      });
      silenceCheck = { headHasAudio, tailHasAudio, headPeak, tailPeak };
    } catch (e) {
      console.error('Silence check failed:', e);
      silenceCheck = null;
    } finally {
      checkingSilence = false;
    }
  }

  // Duration mismatch detection (tolerance of 0.5s)
  let durationDiff = $derived(pair.video ? pair.audio.durationSecs - pair.video.durationSecs : 0);
  let audioLonger = $derived(durationDiff > 0.5);
  let audioShorter = $derived(durationDiff < -0.5);
  let durationWarning = $derived(
    audioLonger
      ? `Audio is ${Math.abs(durationDiff).toFixed(1)}s longer than video — audio will be cut off`
      : audioShorter
        ? `Audio is ${Math.abs(durationDiff).toFixed(1)}s shorter than video — end of video will be silent`
        : null
  );
</script>

<div class="pair-card" class:complete={isComplete} class:failed={isFailed}>
  <!-- Row 1: Source files -->
  <div class="source-row">
    {#if isAudioOnly}
      <!-- Audio-only: waveform icon + audio info -->
      <div class="thumbnail audio-only-thumb">
        <svg width="20" height="14" viewBox="0 0 20 14" fill="none">
          <path d="M1 7H3L5 2L7 12L9 4L11 10L13 5L15 9L17 6H19" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>

      <div class="file-block audio-block" style="flex: 2;">
        <button class="play-btn" onclick={() => previewFile = { path: pair.audio.path, filename: pair.audio.filename, mediaType: 'audio' }} title="Preview audio">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none"><path d="M2 1L9 5L2 9V1Z" fill="currentColor"/></svg>
        </button>
        <span class="file-duration">{formatDuration(pair.audio.durationSecs)}</span>
        <span class="file-name" title="{pair.audio.channelCount ?? '?'}ch {pair.audio.sampleRate ? (pair.audio.sampleRate / 1000).toFixed(1) + 'kHz' : ''}">{pair.audio.filename}</span>
      </div>

      <span class="audio-only-badge">AUDIO ONLY</span>
    {:else}
      <!-- Video + Audio pair -->
      <!-- Video thumbnail -->
      <div class="thumbnail">
        {#if pair.video.thumbnailData}
          <img src={pair.video.thumbnailData} alt="" class="thumb-img" />
        {:else}
          <div class="thumb-placeholder">
            <svg width="16" height="12" viewBox="0 0 16 12" fill="none">
              <rect x="1" y="1" width="14" height="10" rx="1.5" stroke="currentColor" stroke-width="1.2"/>
              <path d="M6 4L10 6L6 8V4Z" fill="currentColor"/>
            </svg>
          </div>
        {/if}
      </div>

      <!-- Video info -->
      <div class="file-block video-block">
        <button class="play-btn" onclick={() => previewFile = { path: pair.video.path, filename: pair.video.filename, mediaType: 'video' }} title="Preview video">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none"><path d="M2 1L9 5L2 9V1Z" fill="currentColor"/></svg>
        </button>
        <span class="file-duration">{formatDuration(pair.video.durationSecs)}</span>
        <span class="file-name" title="{pair.video.codecInfo ?? ''}">{pair.video.filename}</span>
      </div>

      <!-- ProRes working file (Pro Tools) -->
      {#if onCreateProres}
        <ProResButton videoPath={pair.video.path} durationSecs={pair.video.durationSecs} {onCreateProres} {onReveal} />
      {/if}

      <!-- Arrow -->
      <div class="connector" title="Audio will be relayed onto this video">
        <svg width="20" height="14" viewBox="0 0 20 14">
          <path d="M0 7H14M11 3L17 7L11 11" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>

      <!-- Audio info -->
      <div class="file-block audio-block">
        <button class="play-btn" onclick={() => previewFile = { path: pair.audio.path, filename: pair.audio.filename, mediaType: 'audio' }} title="Preview audio">
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none"><path d="M2 1L9 5L2 9V1Z" fill="currentColor"/></svg>
        </button>
        <span class="file-duration">{formatDuration(pair.audio.durationSecs)}</span>
        <span class="file-name" title="{pair.audio.channelCount ?? '?'}ch {pair.audio.sampleRate ? (pair.audio.sampleRate / 1000).toFixed(1) + 'kHz' : ''}">{pair.audio.filename}</span>
      </div>

      <!-- Duration warning -->
      {#if durationWarning}
        <button
          class="duration-warn"
          class:open={showDurationDetail}
          onclick={() => showDurationDetail = !showDurationDetail}
          aria-expanded={showDurationDetail}
          title="Duration mismatch — click for details"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
            <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
          </svg>
        </button>
      {/if}
    {/if}

    <!-- Batch QC result -->
    {#if qcResult?.error}
      <span class="qc-badge fail" title={qcResult.error}>QC &#10007;</span>
    {:else if qcResult}
      {#if qcResult.pass}
        <span class="qc-badge pass" title="QC passed — {qcResult.measuredLufs.toFixed(1)} LUFS / {qcResult.measuredTP.toFixed(1)} dBTP">
          &#10003; {qcResult.measuredLufs.toFixed(1)}
        </span>
      {:else}
        <span class="qc-badge fail" title="QC failed — {qcReason}">
          {qcResult.measuredLufs.toFixed(1)} LUFS
        </span>
        {#if !pair.normalizationEnabled}
          <button class="qc-fix" onclick={fixQc} title="Normalise this file to the batch target when you process">FIX</button>
        {/if}
      {/if}
    {/if}

    <!-- Norm -->
    <div class="norm-section">
      <button
        class="norm-toggle"
        class:active={pair.normalizationEnabled}
        onclick={toggleNorm}
        title={pair.normalizationEnabled ? "Normalization ON — click to disable" : "Click to enable loudness normalization"}
      >
        NORM
      </button>
      {#if pair.normalizationEnabled}
        {#if isAudioOnly}
          <!-- One batch target: the QC bar owns the loudness value, so the
               per-file badge is a read-out, not an editor. -->
          <span class="norm-settings-btn readonly" title="Target set by the QC loudness value">
            {pair.normalizationSettings.targetLufs} LUFS
          </span>
        {:else}
          <button class="norm-settings-btn" onclick={() => showNormSettings = !showNormSettings}
            title="Click to change normalization settings"
          >
            {normMode === 'broadcast' ? `${pair.normalizationSettings.targetLufs} LUFS` : `${pair.normalizationSettings.truePeakLimit} dBTP`}
          </button>
        {/if}
      {/if}
    </div>

    <!-- Silence compliance -->
    <button
      class="silence-toggle"
      class:active={pair.silenceCompliance}
      onclick={toggleCompliance}
      title={pair.silenceCompliance ? "6-frame silence check ON — click to disable" : "Check for 6-frame silence at head/tail (UK broadcast)"}
    >
      6 Fr
    </button>
    {#if pair.silenceCompliance && silenceCheck}
      {#if silenceCheck.headHasAudio || silenceCheck.tailHasAudio}
        <button
          class="silence-warn"
          class:open={showSilenceDetail}
          onclick={() => showSilenceDetail = !showSilenceDetail}
          aria-expanded={showSilenceDetail}
          title="Audio found in the silence guard — click for details"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
            <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
          </svg>
        </button>
      {:else}
        <span class="silence-pass" title="Head and tail silence OK">&#10003;</span>
      {/if}
    {:else if pair.silenceCompliance && checkingSilence}
      <span class="silence-checking">...</span>
    {/if}

    <!-- Clock (audio-only): checks levels + head/tail silence, then adds 10s/5s handles -->
    {#if isAudioOnly && onUpdateClock}
      <button
        class="silence-toggle"
        class:active={pair.clockEnabled}
        onclick={toggleClock}
        disabled={checkingClock}
        title={pair.clockEnabled
          ? '10s head / 5s tail silence will be added — click to disable'
          : 'Check levels & head/tail silence, then add 10s head / 5s tail silence (clock)'}
      >
        Clock
      </button>
      {#if checkingClock}
        <span class="silence-checking">...</span>
      {:else if pair.clockEnabled && clockFailed}
        <!-- Clock on, but the check didn't pass (user proceeded anyway) -->
        <button
          class="silence-warn"
          class:open={showClockDetail}
          onclick={() => showClockDetail = !showClockDetail}
          aria-expanded={showClockDetail}
          title="Clock on despite the check — click for details"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
            <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
          </svg>
        </button>
      {:else if pair.clockEnabled}
        <span class="silence-pass" title="Clock check passed — 10s/5s handles will be added on export">&#10003;</span>
      {:else if clockFailed}
        <button
          class="silence-warn"
          class:open={showClockDetail}
          onclick={() => showClockDetail = !showClockDetail}
          aria-expanded={showClockDetail}
          title="Clock check failed — click for details"
        >
          <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
            <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
            <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
            <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
          </svg>
        </button>
      {/if}
    {/if}

    <!-- Remove -->
    {#if !progress && !result}
      <button class="remove-btn" onclick={() => onRemove(pair.id)} title="Remove pair">&times;</button>
    {/if}
  </div>

  <!-- Row 2: Output name + status -->
  <div class="output-row">
    <span class="output-label">SMART FILENAME</span>
    {#if editingName}
      <div class="name-edit-wrapper">
        <input
          type="text"
          class="name-edit"
          bind:value={editName}
          onkeydown={(e) => {
            if (e.key === 'Enter') saveName();
            if (e.key === 'Escape') cancelEdit();
          }}
          onblur={saveName}
        />
      </div>
    {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <span class="output-name-display" onclick={startEditing}>
        <span class="output-name-text">{outputStem}</span>
        {#if outputExt}
          <span
            class="output-ext"
            title={`This file will be a .${outputExt.toUpperCase()} — the container follows the audio format: Original/WAV → .mov (uncompressed), AAC → .mp4`}
          >.{outputExt}</span>
        {/if}
        <span class="edit-pencil" aria-hidden="true" title="Click the name to rename">
          <svg width="11" height="11" viewBox="0 0 12 12" fill="none">
            <path d="M8.2 1.3 10.7 3.8 4.3 10.2 1.5 10.5 1.8 7.7 8.2 1.3Z" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>
            <path d="M7.2 2.3 9.7 4.8" stroke="currentColor" stroke-width="1.1" stroke-linecap="round"/>
          </svg>
        </span>
        <button class="icon-btn" onclick={addTimestamp} title="Add date & time to filename">
          <svg width="11" height="11" viewBox="0 0 12 12" fill="none">
            <circle cx="6" cy="6" r="5" stroke="currentColor" stroke-width="1.2"/>
            <path d="M6 3V6.5L8.5 8" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
          </svg>
        </button>
      </span>
    {/if}

    <!-- Status -->
    <div class="status-section">
      {#if progress && !result}
        <div class="progress-container">
          <div class="progress-bar">
            <div class="progress-fill" style="width: {progressPct}%"></div>
          </div>
          <span class="progress-label">{progress.message}</span>
        </div>
      {:else if isComplete}
        <div class="complete-actions">
          <button class="finder-btn" onclick={() => onReveal(result.outputPath)} title="Show in Finder">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
              <path d="M2 2H5L6.5 4H12V11H2V2Z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>
            </svg>
            SHOW
          </button>
          <span class="result-badge success">DONE</span>
        </div>
      {:else if isFailed}
        <span class="result-badge error" title={result.error}>FAILED</span>
      {/if}
    </div>
  </div>
</div>

{#if showSilenceDetail && pair.silenceCompliance && silenceCheck && (silenceCheck.headHasAudio || silenceCheck.tailHasAudio)}
  <div class="silence-detail-row">
    <div class="silence-detail-title">
      <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
        <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
        <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
      </svg>
      6-FRAME SILENCE — AUDIO IN THE GUARD REGION
    </div>
    <ul class="silence-detail-list">
      {#if silenceCheck.headHasAudio}
        <li><span class="sd-where">Head</span> — first {(pair.silenceMs ?? 240).toFixed(0)} ms: peak <strong>{silenceCheck.headPeak.toFixed(1)} dBTP</strong></li>
      {/if}
      {#if silenceCheck.tailHasAudio}
        <li><span class="sd-where">Tail</span> — last {(pair.silenceMs ?? 240).toFixed(0)} ms: peak <strong>{silenceCheck.tailPeak.toFixed(1)} dBTP</strong></li>
      {/if}
    </ul>
    <p class="silence-detail-note">On export these regions are muted to digital silence with a {(pair.fadeMs ?? 5).toFixed(0)} ms fade to prevent clicks, so the delivered file is compliant. Nothing to do — this is just a heads-up.</p>
  </div>
{/if}

{#if showDurationDetail && durationWarning}
  <div class="silence-detail-row">
    <div class="silence-detail-title">
      <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
        <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
        <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
      </svg>
      DURATION MISMATCH
    </div>
    <p class="silence-detail-note">{durationWarning}. The output is trimmed to the shorter of the two, so nothing breaks — just check the pairing is right.</p>
  </div>
{/if}

{#if showClockDetail && clockFailed}
  <div class="silence-detail-row">
    <div class="silence-detail-title">
      <svg width="13" height="13" viewBox="0 0 14 14" fill="none">
        <path d="M7 1L13 12H1L7 1Z" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>
        <path d="M7 5.5V8.5" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>
        <circle cx="7" cy="10.5" r="0.5" fill="currentColor"/>
      </svg>
      {pair.clockEnabled ? 'CLOCK ON — PROCEEDING ANYWAY' : 'CLOCK CHECK'}
    </div>
    <ul class="silence-detail-list">
      {#if clockCheck.hasLufsTarget && !clockCheck.lufsPass}
        <li>The level is <strong>{clockCheck.measuredLufs.toFixed(1)} LUFS</strong> — the target is {clockCheck.targetLufs} LUFS (±1).</li>
      {/if}
      {#if !clockCheck.peakPass}
        <li>True peak is <strong>{clockCheck.measuredTP.toFixed(1)} dBTP</strong> — over the {clockCheck.truePeakLimit} dBTP ceiling.</li>
      {/if}
      {#if !clockCheck.silencePass}
        <li>Audio found in the {clockCheck.headHasAudio ? 'head' : ''}{clockCheck.headHasAudio && clockCheck.tailHasAudio ? ' &amp; ' : ''}{clockCheck.tailHasAudio ? 'tail' : ''} silence guard.</li>
      {/if}
    </ul>
    {#if pair.clockEnabled}
      <p class="silence-detail-note">Proceeding anyway — the 10s / 5s handles will be added on export. Click <strong>Clock</strong> to turn it back off.</p>
    {:else}
      <p class="silence-detail-note">This file isn't at the target, but you can clock it anyway.</p>
      <button class="clock-proceed" onclick={proceedClock}>Proceed anyway &rarr;</button>
    {/if}
  </div>
{/if}

{#if showNormSettings && pair.normalizationEnabled && !isAudioOnly}
  <div class="norm-detail-row">
    <!-- Mode selector -->
    <div class="norm-mode-switch">
      <button
        class="mode-btn"
        class:active={normMode === 'broadcast'}
        onclick={() => normMode = 'broadcast'}
      >BROADCAST / STREAMING</button>
      <button
        class="mode-btn"
        class:active={normMode === 'fullscale'}
        onclick={() => normMode = 'fullscale'}
      >FULL SCALE REVIEW</button>
    </div>

    {#if normMode === 'broadcast'}
      <!-- Broadcast mode: LUFS target with true peak safety -->
      <div class="norm-options">
        <div class="norm-presets">
          <button class="preset-btn" class:active={pair.normalizationSettings.targetLufs === -16}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: -16, truePeakLimit: -1 })}
          >-16 LUFS<span class="preset-sub">Streaming</span></button>
          <button class="preset-btn" class:active={pair.normalizationSettings.targetLufs === -23}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: -23, truePeakLimit: -1 })}
          >-23 LUFS<span class="preset-sub">EBU R128</span></button>
          <button class="preset-btn" class:active={pair.normalizationSettings.targetLufs === -24}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: -24, truePeakLimit: -2 })}
          >-24 LUFS<span class="preset-sub">ATSC A/85</span></button>
        </div>
        <div class="custom-row">
          <label class="custom-input">
            <span class="custom-label">LUFS</span>
            <input type="number" step="0.5" min="-40" max="-5"
              value={pair.normalizationSettings.targetLufs}
              onchange={(e) => onUpdateNormalization(pair.id, true, { ...pair.normalizationSettings, targetLufs: parseFloat(e.target.value) })}
            />
          </label>
          <label class="custom-input">
            <span class="custom-label">TP LIMIT</span>
            <input type="number" step="0.1" min="-6" max="0"
              value={pair.normalizationSettings.truePeakLimit}
              onchange={(e) => onUpdateNormalization(pair.id, true, { ...pair.normalizationSettings, truePeakLimit: parseFloat(e.target.value) })}
            />
            <span class="unit">dBTP</span>
          </label>
        </div>
      </div>
    {:else}
      <!-- Full scale mode: just true peak limit, no LUFS target -->
      <div class="norm-options">
        <p class="mode-description">Normalize to maximum level, limited by true peak.</p>
        <div class="norm-presets">
          <button class="preset-btn" class:active={pair.normalizationSettings.truePeakLimit === -1}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: 0, truePeakLimit: -1 })}
          >-1 dBTP<span class="preset-sub">Standard</span></button>
          <button class="preset-btn" class:active={pair.normalizationSettings.truePeakLimit === -0.5}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: 0, truePeakLimit: -0.5 })}
          >-0.5 dBTP<span class="preset-sub">Tight</span></button>
          <button class="preset-btn" class:active={pair.normalizationSettings.truePeakLimit === -2}
            onclick={() => onUpdateNormalization(pair.id, true, { targetLufs: 0, truePeakLimit: -2 })}
          >-2 dBTP<span class="preset-sub">Safe</span></button>
        </div>
        <label class="custom-input">
          <span class="custom-label">TP LIMIT</span>
          <input type="number" step="0.1" min="-6" max="0"
            value={pair.normalizationSettings.truePeakLimit}
            onchange={(e) => onUpdateNormalization(pair.id, true, { targetLufs: 0, truePeakLimit: parseFloat(e.target.value) })}
          />
          <span class="unit">dBTP</span>
        </label>
      </div>
    {/if}

    {#if result?.measuredLufs != null}
      <div class="norm-measured">
        MEASURED: {result.measuredLufs.toFixed(1)} LUFS / {result.measuredTruePeak.toFixed(1)} dBTP
      </div>
    {/if}
  </div>
{/if}

{#if previewFile}
  <MediaPreview
    filePath={previewFile.path}
    filename={previewFile.filename}
    mediaType={previewFile.mediaType}
    onClose={() => previewFile = null}
  />
{/if}

<style>
  /* === Card container === */
  .pair-card {
    background: var(--bg-panel);
    border: 1px solid var(--border-color);
    border-radius: var(--radius-md);
    transition: all 0.2s;
    overflow: hidden;
    flex-shrink: 0;
  }

  .pair-card:hover {
    background: var(--bg-panel-hover);
    border-color: var(--border-accent);
  }

  .pair-card.complete {
    border-color: rgba(57, 255, 20, 0.3);
  }

  .pair-card.failed {
    border-color: rgba(255, 46, 99, 0.3);
  }

  :global(:root.tame) .pair-card.complete {
    border-color: rgba(106, 154, 90, 0.4);
  }

  :global(:root.tame) .pair-card.failed {
    border-color: rgba(224, 122, 95, 0.4);
  }

  /* === Row 1: Source files === */
  .source-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px var(--gap-md) 6px;
  }

  /* Thumbnail */
  .thumbnail {
    flex-shrink: 0;
    width: 60px;
    height: 40px;
    border-radius: 3px;
    overflow: hidden;
    background: var(--bg-dark);
    border: 1px solid var(--border-color);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .thumb-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb-placeholder {
    color: var(--text-muted);
    opacity: 0.4;
  }

  .audio-only-thumb {
    background: rgba(8, 247, 254, 0.06);
    border-color: rgba(8, 247, 254, 0.2);
    color: var(--neon-cyan);
    opacity: 0.7;
  }

  .audio-only-badge {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--neon-cyan);
    background: rgba(8, 247, 254, 0.08);
    border: 1px solid rgba(8, 247, 254, 0.25);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    flex-shrink: 0;
    white-space: nowrap;
  }

  /* File block: duration + name inline */
  .file-block {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .play-btn {
    flex-shrink: 0;
    background: none;
    border: 1px solid var(--border-color);
    color: var(--text-muted);
    cursor: pointer;
    padding: 3px 4px;
    border-radius: 3px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.15s;
    opacity: 0.5;
    align-self: center;
  }

  .play-btn:hover {
    opacity: 1;
    color: var(--neon-cyan);
    border-color: rgba(8, 247, 254, 0.4);
    background: rgba(8, 247, 254, 0.08);
  }

  .file-duration {
    font-family: var(--font-mono);
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
    flex-shrink: 0;
  }

  .file-name {
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    word-break: break-all;
    line-height: 1.3;
  }

  .video-block .file-name {
    color: var(--text-primary);
  }

  .connector {
    color: var(--text-muted);
    flex-shrink: 0;
    opacity: 0.4;
  }

  /* ProRes working-file button — subtle by default */

  .duration-warn {
    flex-shrink: 0;
    color: var(--neon-orange);
    display: flex;
    align-items: center;
    background: none;
    border: none;
    padding: 2px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    opacity: 0.8;
    transition: all 0.15s;
  }

  .duration-warn:hover,
  .duration-warn.open {
    opacity: 1;
    background: rgba(255, 159, 28, 0.12);
  }

  .norm-section {
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
    flex-shrink: 0;
  }

  .norm-toggle {
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    border: 2px solid var(--border-accent);
    background: var(--bg-dark);
    color: var(--text-muted);
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    cursor: pointer;
    transition: all 0.2s;
  }

  .norm-toggle:hover {
    border-color: var(--neon-yellow);
    color: var(--text-secondary);
  }

  .norm-toggle.active {
    border-color: var(--neon-yellow);
    color: var(--bg-dark);
    background: var(--neon-yellow);
    box-shadow: 0 0 8px rgba(237, 255, 33, 0.3);
  }

  .norm-settings-btn {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--neon-yellow);
    background: rgba(237, 255, 33, 0.08);
    border: 1px solid rgba(237, 255, 33, 0.3);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 90px;
    transition: all 0.15s;
    white-space: nowrap;
  }

  .norm-settings-btn:hover {
    background: rgba(237, 255, 33, 0.15);
  }

  .norm-settings-btn.readonly {
    cursor: default;
  }
  .norm-settings-btn.readonly:hover {
    background: rgba(237, 255, 33, 0.08);
  }

  .silence-toggle {
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    border: 2px solid var(--border-accent);
    background: var(--bg-dark);
    color: var(--text-muted);
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.05em;
    cursor: pointer;
    transition: all 0.2s;
    flex-shrink: 0;
  }

  .silence-toggle:hover {
    border-color: var(--neon-orange);
    color: var(--text-secondary);
  }

  .silence-toggle.active {
    border-color: var(--neon-orange);
    color: var(--bg-dark);
    background: var(--neon-orange);
    box-shadow: 0 0 8px rgba(255, 110, 39, 0.3);
  }

  .silence-warn {
    flex-shrink: 0;
    color: var(--neon-orange);
    display: flex;
    align-items: center;
    background: none;
    border: none;
    padding: 2px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: all 0.15s;
  }
  .silence-warn:hover,
  .silence-warn.open {
    background: rgba(255, 159, 28, 0.12);
  }

  .silence-pass {
    flex-shrink: 0;
    color: var(--neon-green);
    font-size: 14px;
    font-weight: 700;
  }

  .silence-checking {
    flex-shrink: 0;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: 10px;
  }

  .remove-btn {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 19px;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: var(--radius-sm);
    line-height: 1;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    color: var(--neon-pink);
    background: rgba(255, 46, 99, 0.1);
  }

  :global(:root.tame) .remove-btn:hover {
    background: rgba(224, 122, 95, 0.12);
  }

  /* === Row 2: Output name + status === */
  .output-row {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
    padding: 4px var(--gap-md) 8px;
    border-top: 1px solid var(--border-color);
    margin: 0 4px;
    min-height: 40px;
    height: 40px;
  }

  .output-label {
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.12em;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .output-name-display {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-width: 0;
    padding: 4px 10px;
    height: 30px;
    box-sizing: border-box;
    border: 1px solid rgba(8, 247, 254, 0.25);
    border-radius: var(--radius-sm);
    transition: all 0.15s;
    background: rgba(8, 247, 254, 0.04);
    cursor: text;
    min-height: 22px;
    box-sizing: border-box;
  }

  .output-name-display:hover {
    border-color: rgba(8, 247, 254, 0.4);
    background: rgba(8, 247, 254, 0.05);
    box-shadow: 0 0 8px rgba(8, 247, 254, 0.08);
  }

  .output-name-text {
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 700;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex: 0 1 auto;
    min-width: 0;
    line-height: 1.3;
  }

  .output-ext {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 14px;
    font-weight: 700;
    color: var(--neon-cyan);
    line-height: 1.3;
  }

  :global(:root.tame) .output-ext {
    color: var(--neon-green);
  }

  .output-name-display:hover .output-name-text {
    color: var(--text-primary);
  }

  .icon-btn {
    flex-shrink: 0;
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
    border-radius: 2px;
    opacity: 0;
    transition: all 0.15s;
    display: flex;
    align-items: center;
  }

  .output-name-display:hover .icon-btn {
    opacity: 0.5;
  }

  .icon-btn:hover {
    opacity: 1 !important;
    color: var(--neon-cyan);
  }

  /* Permanent pencil so it's obvious the filename can be renamed. */
  .edit-pencil {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    color: var(--text-muted);
    opacity: 0.55;
    transition: all 0.15s;
  }

  .output-name-display:hover .edit-pencil {
    opacity: 1;
    color: var(--neon-cyan);
  }

  .name-edit-wrapper {
    flex: 1;
    min-width: 0;
  }

  .name-edit {
    width: 100%;
    font-size: 14px;
    font-weight: 700;
    font-family: var(--font-mono);
    padding: 4px 10px;
    height: 30px;
    box-sizing: border-box;
    background: var(--bg-dark);
    border: 1px solid var(--neon-cyan);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    outline: none;
    box-shadow: 0 0 6px rgba(8, 247, 254, 0.15);
    min-height: 22px;
    box-sizing: border-box;
    line-height: 1.3;
  }

  /* === Status / Actions === */
  .status-section {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }

  .progress-container {
    display: flex;
    align-items: center;
    gap: var(--gap-xs);
  }

  .progress-bar {
    width: 50px;
    height: 4px;
    background: var(--bg-dark);
    border-radius: 2px;
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--neon-cyan);
    transition: width 0.3s ease;
    border-radius: 2px;
  }

  .progress-label {
    font-family: var(--font-mono);
    font-size: 10px;
    color: var(--text-muted);
    white-space: nowrap;
  }

  .complete-actions {
    display: flex;
    align-items: center;
    gap: var(--gap-sm);
  }

  .finder-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--neon-cyan);
    background: rgba(8, 247, 254, 0.08);
    border: 1px solid rgba(8, 247, 254, 0.25);
    border-radius: var(--radius-sm);
    padding: 4px 8px;
    cursor: pointer;
    transition: all 0.15s;
  }

  .finder-btn:hover {
    background: rgba(8, 247, 254, 0.15);
    border-color: var(--neon-cyan);
    box-shadow: 0 0 8px rgba(8, 247, 254, 0.2);
  }

  .finder-btn svg {
    flex-shrink: 0;
  }

  .result-badge {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    padding: 4px 10px;
    border-radius: var(--radius-sm);
    letter-spacing: 0.1em;
    border: none;
  }

  .result-badge.success {
    background: rgba(57, 255, 20, 0.15);
    color: var(--neon-green);
  }

  .result-badge.error {
    background: rgba(255, 46, 99, 0.15);
    color: var(--neon-pink);
  }

  :global(:root.tame) .result-badge.success {
    background: rgba(106, 154, 90, 0.15);
  }

  :global(:root.tame) .result-badge.error {
    background: rgba(224, 122, 95, 0.15);
  }

  /* === Normalization detail panel === */
  .norm-detail-row {
    display: flex;
    flex-direction: column;
    gap: var(--gap-md);
    padding: var(--gap-md) var(--gap-lg);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    background: rgba(237, 255, 33, 0.03);
    border: 1px solid rgba(237, 255, 33, 0.15);
    border-top: none;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    margin-top: -4px;
  }

  /* === 6-frame silence detail panel === */
  .silence-detail-row {
    display: flex;
    flex-direction: column;
    gap: var(--gap-xs);
    padding: var(--gap-md) var(--gap-lg);
    font-family: var(--font-mono);
    font-size: 12px;
    color: var(--text-secondary);
    background: rgba(255, 159, 28, 0.05);
    border: 1px solid rgba(255, 159, 28, 0.2);
    border-top: none;
    border-radius: 0 0 var(--radius-md) var(--radius-md);
    margin-top: -4px;
  }
  .silence-detail-title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.08em;
    color: var(--neon-orange);
  }
  .silence-detail-list {
    margin: 2px 0 0;
    padding-left: 18px;
    list-style: none;
  }
  .silence-detail-list li { padding: 1px 0; }
  .silence-detail-list .sd-where {
    color: var(--neon-orange);
    font-weight: 700;
  }
  .silence-detail-list strong { color: var(--text-primary); }
  .silence-detail-note {
    margin: 4px 0 0;
    color: var(--text-muted);
    line-height: 1.5;
  }

  /* ── Batch QC badge ── */
  .qc-badge {
    flex-shrink: 0;
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.03em;
    border-radius: var(--radius-sm);
    padding: 3px 7px;
    white-space: nowrap;
    cursor: help;
  }
  .qc-badge.pass {
    color: var(--neon-green);
    background: rgba(57, 255, 20, 0.08);
    border: 1px solid rgba(57, 255, 20, 0.3);
  }
  .qc-badge.fail {
    color: var(--neon-orange);
    background: rgba(255, 159, 28, 0.1);
    border: 1px solid rgba(255, 159, 28, 0.4);
  }

  .qc-fix {
    flex-shrink: 0;
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.1em;
    color: var(--neon-orange);
    background: none;
    border: 1px solid rgba(255, 159, 28, 0.4);
    border-radius: var(--radius-sm);
    padding: 3px 7px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .qc-fix:hover {
    background: rgba(255, 159, 28, 0.18);
    color: var(--text-primary);
  }

  .clock-proceed {
    margin-top: 8px;
    align-self: flex-start;
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: var(--neon-orange);
    background: rgba(255, 159, 28, 0.1);
    border: 1px solid rgba(255, 159, 28, 0.4);
    border-radius: var(--radius-sm);
    padding: 5px 12px;
    cursor: pointer;
    transition: all 0.15s;
  }
  .clock-proceed:hover {
    background: rgba(255, 159, 28, 0.18);
    color: var(--text-primary);
  }

  /* Mode switch (BROADCAST vs FULL SCALE) */
  .norm-mode-switch {
    display: flex;
    gap: 2px;
    background: var(--bg-dark);
    border-radius: var(--radius-sm);
    padding: 2px;
    border: 1px solid var(--border-accent);
    width: fit-content;
  }

  .mode-btn {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    padding: 6px 14px;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition: all 0.15s;
  }

  .mode-btn:hover {
    color: var(--text-secondary);
  }

  .mode-btn.active {
    background: rgba(237, 255, 33, 0.15);
    color: var(--neon-yellow);
    box-shadow: 0 0 6px rgba(237, 255, 33, 0.1);
  }

  /* Options area (presets + custom inputs) */
  .norm-options {
    display: flex;
    flex-direction: column;
    gap: var(--gap-sm);
  }

  .mode-description {
    font-size: 11px;
    color: var(--text-muted);
    margin: 0;
    font-style: italic;
  }

  .norm-presets {
    display: flex;
    gap: 4px;
  }

  .preset-btn {
    font-family: var(--font-mono);
    font-size: 12px;
    font-weight: 700;
    padding: 6px 12px;
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-sm);
    background: var(--bg-dark);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all 0.15s;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2px;
  }

  .preset-btn:hover {
    border-color: var(--neon-yellow);
    color: var(--text-primary);
  }

  .preset-btn.active {
    border-color: var(--neon-yellow);
    background: rgba(237, 255, 33, 0.15);
    color: var(--neon-yellow);
    box-shadow: 0 0 6px rgba(237, 255, 33, 0.15);
  }

  .preset-sub {
    font-size: 9px;
    font-weight: 400;
    opacity: 0.6;
    letter-spacing: 0.05em;
    text-transform: uppercase;
  }

  /* Custom input rows */
  .custom-row {
    display: flex;
    gap: var(--gap-md);
    align-items: center;
  }

  .custom-input {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .custom-label {
    font-family: var(--font-display);
    font-size: 10px;
    letter-spacing: 0.1em;
    color: var(--neon-yellow);
    opacity: 0.7;
    white-space: nowrap;
  }

  .custom-input input[type="number"] {
    width: 65px;
    font-size: 12px;
    font-family: var(--font-mono);
    padding: 4px 6px;
    background: var(--bg-dark);
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    outline: none;
    transition: border-color 0.15s;
  }

  .custom-input input[type="number"]:focus {
    border-color: var(--neon-yellow);
    box-shadow: 0 0 6px rgba(237, 255, 33, 0.15);
  }

  .unit {
    color: var(--text-muted);
    font-size: 11px;
  }

  /* Measured result display */
  .norm-measured {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--neon-green);
    padding: var(--gap-xs) var(--gap-sm);
    background: rgba(57, 255, 20, 0.05);
    border: 1px solid rgba(57, 255, 20, 0.15);
    border-radius: var(--radius-sm);
    width: fit-content;
  }

  :global(:root.tame) .norm-settings-btn {
    color: #8b6914;
    background: rgba(201, 162, 39, 0.12);
    border-color: rgba(201, 162, 39, 0.4);
    font-weight: 600;
  }

  :global(:root.tame) .norm-detail-row {
    background: rgba(201, 162, 39, 0.06);
    border-color: rgba(201, 162, 39, 0.2);
  }

  :global(:root.tame) .norm-measured {
    background: rgba(106, 154, 90, 0.08);
    border-color: rgba(106, 154, 90, 0.2);
  }
</style>
